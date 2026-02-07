use std::{sync::Arc, time::Duration};

use anyhow::Context;
use format_serde_error::SerdeError;
use http::HeaderMap;
use reqwest_middleware::{
    ClientBuilder, ClientWithMiddleware, Error as MiddlewareError, Extension,
};
// Use the reqwest version re-exported by reqwest-middleware (reqwest 0.13)
// to ensure type compatibility with the middleware client.
// The workspace `reqwest` dep is 0.12, used by openidconnect, opentelemetry, etc.
use reqwest_middleware::reqwest::{IntoUrl, Response, StatusCode};
use reqwest_retry::{
    Jitter, RetryTransientMiddleware, Retryable, RetryableStrategy, default_on_request_failure,
    default_on_request_success, policies::ExponentialBackoff,
};
use reqwest_tracing::{
    DisableOtelPropagation, OtelPathNames, SpanBackendWithUrl, TracingMiddleware,
};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::universal_inbox::UniversalInboxError;

pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Error, Debug)]
pub enum ApiClientError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest_middleware::reqwest::Error),

    #[error("Rate limit exceeded: {message}")]
    RateLimitError { message: String },

    #[error("Middleware error: {0}")]
    MiddlewareError(#[from] MiddlewareError),

    #[error("Json parsing error: {0}")]
    JsonParsingError(#[from] SerdeError),

    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl ApiClientError {
    pub fn rate_limit_error(message: String) -> Self {
        Self::RateLimitError { message }
    }

    pub fn from_json_serde_error(serde_error: serde_json::Error, input: String) -> Self {
        ApiClientError::JsonParsingError(SerdeError::new(input.clone(), serde_error))
    }
}

// Trait for detecting rate limits from HTTP responses
pub trait RateLimitDetector: Send + Sync {
    /// Check if the response indicates a rate limit
    fn is_rate_limit_response(&self, response: &Response) -> bool;
}

// Default implementation that checks for 429 status code
#[derive(Debug, Clone)]
pub struct DefaultRateLimitDetector;

impl RateLimitDetector for DefaultRateLimitDetector {
    fn is_rate_limit_response(&self, response: &Response) -> bool {
        response.status() == StatusCode::TOO_MANY_REQUESTS
    }
}

struct RateLimitRetryableStrategy {
    rate_limit_detector: Arc<dyn RateLimitDetector>,
}

impl RetryableStrategy for RateLimitRetryableStrategy {
    fn handle(&self, res: &Result<Response, reqwest_middleware::Error>) -> Option<Retryable> {
        match res {
            Ok(success) if self.rate_limit_detector.is_rate_limit_response(success) => {
                Some(Retryable::Transient)
            }
            // otherwise fallback on default retry behavior
            Ok(success) => default_on_request_success(success),
            // but maybe retry a request failure
            Err(error) => default_on_request_failure(error),
        }
    }
}

pub struct ApiClient {
    rate_limit_detector: Arc<dyn RateLimitDetector>,
    client: ClientWithMiddleware,
}

impl ApiClient {
    pub fn build<Paths, Path>(
        default_headers: HeaderMap,
        known_paths: Paths,
        max_retry_duration: Duration,
    ) -> Result<Self, UniversalInboxError>
    where
        Paths: IntoIterator<Item = Path>,
        Path: Into<String>,
    {
        ApiClient::with_rate_limit_detector(
            default_headers,
            known_paths,
            max_retry_duration,
            Arc::new(DefaultRateLimitDetector),
        )
    }

    pub fn with_rate_limit_detector<Paths, Path>(
        default_headers: HeaderMap,
        known_paths: Paths,
        max_retry_duration: Duration,
        rate_limit_detector: Arc<dyn RateLimitDetector>,
    ) -> Result<Self, UniversalInboxError>
    where
        Paths: IntoIterator<Item = Path>,
        Path: Into<String>,
    {
        let mut client_builder = ClientBuilder::new(
            reqwest_middleware::reqwest::Client::builder()
                .default_headers(default_headers)
                .user_agent(APP_USER_AGENT)
                .build()
                .context("Cannot build client")?,
        )
        .with_init(Extension(
            OtelPathNames::known_paths(known_paths).context("Cannot build Otel path names")?,
        ))
        .with_init(Extension(DisableOtelPropagation));

        if max_retry_duration.as_secs() > 0 {
            let rate_limit_retry_policy = ExponentialBackoff::builder()
                .retry_bounds(Duration::from_secs(1), Duration::from_secs(60))
                .jitter(Jitter::Bounded)
                .base(2)
                .build_with_total_retry_duration(max_retry_duration);
            let rate_limit_retry_middleware =
                RetryTransientMiddleware::new_with_policy_and_strategy(
                    rate_limit_retry_policy,
                    RateLimitRetryableStrategy {
                        rate_limit_detector: rate_limit_detector.clone(),
                    },
                );

            client_builder = client_builder.with(rate_limit_retry_middleware);
        }

        client_builder = client_builder.with(TracingMiddleware::<SpanBackendWithUrl>::new());

        Ok(Self {
            rate_limit_detector,
            client: client_builder.build(),
        })
    }

    async fn handle_response(
        &self,
        response: Result<Response, reqwest_middleware::Error>,
    ) -> Result<String, ApiClientError> {
        let response = response.map_err(ApiClientError::MiddlewareError)?;

        if self.rate_limit_detector.is_rate_limit_response(&response) {
            return Err(ApiClientError::rate_limit_error(
                "Too many requests".to_string(),
            ));
        }

        response
            .error_for_status()
            .map_err(ApiClientError::NetworkError)?
            .text()
            .await
            .map_err(ApiClientError::NetworkError)
    }

    pub async fn get<R: DeserializeOwned, U: IntoUrl>(&self, url: U) -> Result<R, ApiClientError> {
        let response_body = self
            .handle_response(self.client.get(url).send().await)
            .await?;

        serde_json::from_str(&response_body)
            .map_err(|err| ApiClientError::from_json_serde_error(err, response_body.clone()))
    }

    pub async fn post<R: DeserializeOwned, U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<R, ApiClientError> {
        let mut response_builder = self.client.post(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        let response_body = self.handle_response(response).await?;

        serde_json::from_str(&response_body)
            .map_err(|err| ApiClientError::from_json_serde_error(err, response_body.clone()))
    }

    pub async fn post_no_response<U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<(), ApiClientError> {
        let mut response_builder = self.client.post(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        self.handle_response(response).await?;

        Ok(())
    }

    pub async fn post_form<R: DeserializeOwned, U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: &T,
    ) -> Result<R, ApiClientError> {
        let response_body = self
            .handle_response(self.client.post(url).form(request_body).send().await)
            .await?;

        serde_json::from_str(&response_body)
            .map_err(|err| ApiClientError::from_json_serde_error(err, response_body.clone()))
    }

    pub async fn patch<R: DeserializeOwned, U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<R, ApiClientError> {
        let mut response_builder = self.client.patch(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        let response_body = self.handle_response(response).await?;

        serde_json::from_str(&response_body)
            .map_err(|err| ApiClientError::from_json_serde_error(err, response_body.clone()))
    }

    pub async fn patch_no_response<U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<(), ApiClientError> {
        let mut response_builder = self.client.patch(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        self.handle_response(response).await?;

        Ok(())
    }

    pub async fn put<R: DeserializeOwned, U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<R, ApiClientError> {
        let mut response_builder = self.client.put(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        let response_body = self.handle_response(response).await?;

        serde_json::from_str(&response_body)
            .map_err(|err| ApiClientError::from_json_serde_error(err, response_body.clone()))
    }

    pub async fn put_no_response<U: IntoUrl, T: Serialize + ?Sized>(
        &self,
        url: U,
        request_body: Option<&T>,
    ) -> Result<(), ApiClientError> {
        let mut response_builder = self.client.put(url);
        if let Some(body) = request_body {
            response_builder = response_builder.json(body);
        }
        let response = response_builder.send().await;

        self.handle_response(response).await?;

        Ok(())
    }

    pub async fn delete_no_response<U: IntoUrl>(&self, url: U) -> Result<(), ApiClientError> {
        let response = self.client.delete(url).send().await;

        self.handle_response(response).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests_api_error {
    use super::*;
    use rstest::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    // Mock rate limit detector for testing
    #[derive(Debug)]
    struct MockRateLimitDetector {
        should_trigger: bool,
    }

    impl RateLimitDetector for MockRateLimitDetector {
        fn is_rate_limit_response(&self, _response: &Response) -> bool {
            self.should_trigger
        }
    }

    #[fixture]
    async fn mock_server() -> MockServer {
        MockServer::start().await
    }

    async fn mock_api_call(mock_server: &MockServer, response_status: u16) {
        let response = if response_status == 200 {
            ResponseTemplate::new(response_status).set_body_json(serde_json::json!({
                "message": "Success"
            }))
        } else {
            ResponseTemplate::new(response_status).set_body_string("Error")
        };
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(response)
            .mount(mock_server)
            .await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_api_client_with_default_detector(#[future] mock_server: MockServer) {
        let mock_server = mock_server.await;
        mock_api_call(&mock_server, 200).await;
        let url = format!("{}/", mock_server.uri());
        let api_client =
            ApiClient::build(HeaderMap::new(), vec![url.clone()], Duration::from_secs(1)).unwrap();

        let result: Result<serde_json::Value, ApiClientError> = api_client.get(url).await;
        println!("Result: {:?}", result);
        assert!(result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn test_api_client_rate_limited(#[future] mock_server: MockServer) {
        let mock_server = mock_server.await;
        mock_api_call(&mock_server, 429).await;
        let url = format!("{}/", mock_server.uri());
        let api_client =
            ApiClient::build(HeaderMap::new(), vec![url.clone()], Duration::from_secs(1)).unwrap();

        let result: Result<serde_json::Value, ApiClientError> = api_client.get(url).await;
        assert!(matches!(result, Err(ApiClientError::RateLimitError { .. })));
    }

    #[rstest]
    #[tokio::test]
    async fn test_api_client_network_error(#[future] mock_server: MockServer) {
        let mock_server = mock_server.await;
        mock_api_call(&mock_server, 500).await;
        let url = format!("{}/", mock_server.uri());
        let api_client =
            ApiClient::build(HeaderMap::new(), vec![url.clone()], Duration::from_secs(1)).unwrap();

        let result: Result<serde_json::Value, ApiClientError> = api_client.get(url).await;
        assert!(matches!(result, Err(ApiClientError::NetworkError(_))));
    }

    #[rstest]
    #[tokio::test]
    async fn test_custom_rate_limit_detector(#[future] mock_server: MockServer) {
        // Test with a detector that never triggers
        let non_triggering_detector = MockRateLimitDetector {
            should_trigger: false,
        };
        let mock_server = mock_server.await;
        mock_api_call(&mock_server, 429).await;
        let url = format!("{}/", mock_server.uri());
        let api_client = ApiClient::with_rate_limit_detector(
            HeaderMap::new(),
            vec![url.clone()],
            Duration::from_secs(1),
            Arc::new(non_triggering_detector),
        )
        .unwrap();

        let result: Result<serde_json::Value, ApiClientError> = api_client.get(url).await;
        assert!(matches!(result, Err(ApiClientError::NetworkError(_))));
    }
}
