use anyhow::{anyhow, Context, Result};
use openidconnect::{
    core::{
        CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreClient, CoreErrorResponseType,
        CoreGenderClaim, CoreJsonWebKey, CoreJsonWebKeyType, CoreJsonWebKeyUse,
        CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm, CoreProviderMetadata,
        CoreRevocableToken, CoreTokenType,
    },
    reqwest::async_http_client,
    url::Url,
    AccessToken, AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EmptyAdditionalClaims, EmptyExtraTokenFields, IdToken, IdTokenClaims, IdTokenFields, IssuerUrl,
    Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    RevocationErrorResponseType, StandardErrorResponse, StandardTokenIntrospectionResponse,
    StandardTokenResponse, TokenResponse,
};

pub type OpenidConnectClient = openidconnect::Client<
    EmptyAdditionalClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
    CoreJsonWebKeyType,
    CoreJsonWebKeyUse,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    StandardTokenResponse<
        IdTokenFields<
            EmptyAdditionalClaims,
            EmptyExtraTokenFields,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
        CoreTokenType,
    >,
    CoreTokenType,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, CoreTokenType>,
    CoreRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
>;

pub struct OpenidConnectProvider {
    pub client: OpenidConnectClient,
}

impl OpenidConnectProvider {
    pub async fn build(
        issuer_url: IssuerUrl,
        client_id: ClientId,
        client_secret: Option<ClientSecret>,
        redirect_url: RedirectUrl,
    ) -> Result<OpenidConnectProvider> {
        // Issuer URL must strictly be equal to the one found in the auth provider
        // metadata. For now clearly the trailing slash added by Url.to_string().
        let issuer_url_string = issuer_url.as_str().trim_end_matches('/').to_string();
        // Use OpenID Connect Discovery to fetch the provider metadata.
        let provider_metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(issuer_url_string)
                .context("Failed to build OpenID Connect issuer URL")?,
            async_http_client,
        )
        .await
        .context("Failed to discover OpenID Connect provider metadata")?;

        // Create an OpenID Connect client by specifying the client ID
        let client =
            CoreClient::from_provider_metadata(provider_metadata, client_id, client_secret)
                // Set the URL the user will be redirected to after the authorization process.
                .set_redirect_uri(redirect_url);

        Ok(OpenidConnectProvider { client })
    }

    pub fn build_authorization_code_pkce_flow_auth_url(
        &self,
        pkce_code_challenge: PkceCodeChallenge,
    ) -> (Url, CsrfToken, Nonce) {
        self.client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(openidconnect::Scope::new("profile".to_string()))
            .add_scope(openidconnect::Scope::new("email".to_string()))
            .set_pkce_challenge(pkce_code_challenge)
            .url()
    }

    pub fn build_google_authorization_code_flow_auth_url(&self) -> (Url, CsrfToken, Nonce) {
        self.client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(openidconnect::Scope::new("profile".to_string()))
            .add_scope(openidconnect::Scope::new("email".to_string()))
            .add_extra_param("prompt", "select_account")
            .url()
    }

    pub async fn fetch_access_token(
        &self,
        auth_code: AuthorizationCode,
        nonce: Nonce,
        pkce_code_verifier: Option<PkceCodeVerifier>,
    ) -> Result<(
        AccessToken,
        IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
    )> {
        let mut token_request = self.client.exchange_code(auth_code);
        if let Some(pkce_code_verifier) = pkce_code_verifier {
            token_request = token_request.set_pkce_verifier(pkce_code_verifier);
        }
        let token_response = token_request
            .request_async(async_http_client)
            .await
            .map_err(|err| {
                anyhow!(
                    "Failed to get OpenID Connect access token in exchange of the auth code: {:?}",
                    err.to_string()
                )
            })?;

        // Extract the ID token claims after verifying its authenticity
        let id_token = token_response
            .id_token()
            .context("Server did not return an ID token")?;
        let claims = self.verify_id_token_claims(id_token, &nonce)?;

        let access_token = token_response.access_token();
        // Verify the access token hash to ensure that the access token hasn't been substituted for
        // another user's.
        if let Some(expected_access_token_hash) = claims.access_token_hash() {
            let actual_access_token_hash = AccessTokenHash::from_token(
                access_token,
                &id_token
                    .signing_alg()
                    .context("OpenID connect auth ID token is not signed")?,
            )
            .context("Failed to hash access token")?;
            if actual_access_token_hash != *expected_access_token_hash {
                return Err(anyhow!("Invalid access token: Access token hash mismatch"));
            }
        }

        Ok((access_token.clone(), id_token.clone()))
    }

    pub fn verify_id_token_claims<'a>(
        &'a self,
        id_token: &'a IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
        nonce: &'a Nonce,
    ) -> Result<&'a IdTokenClaims<EmptyAdditionalClaims, CoreGenderClaim>> {
        id_token
            .claims(
                &self
                    .client
                    .id_token_verifier()
                    .set_other_audience_verifier_fn(|_| true),
                nonce,
            )
            .context("Failed to verify OpenID Connect auth ID token")
    }
}
