use reqwest::Response;
use uuid::Uuid;

pub async fn get_resource_response(
    app_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Response {
    reqwest::Client::new()
        .get(&format!("{app_address}/{resource_name}/{id}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn get_resource<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(
    app_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Box<T> {
    get_resource_response(app_address, resource_name, id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn create_resource_response<T: serde::Serialize>(
    app_address: &str,
    resource_name: &str,
    resource: Box<T>,
) -> Response {
    reqwest::Client::new()
        .post(&format!("{app_address}/{resource_name}"))
        .json(&*resource)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_resource<T: serde::Serialize, U: for<'a> serde::Deserialize<'a>>(
    app_address: &str,
    resource_name: &str,
    resource: Box<T>,
) -> Box<U> {
    create_resource_response(app_address, resource_name, resource)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn patch_resource_response<P: serde::Serialize>(
    app_address: &str,
    resource_name: &str,
    id: Uuid,
    patch: &P,
) -> Response {
    reqwest::Client::new()
        .patch(&format!("{app_address}/{resource_name}/{id}"))
        .json(patch)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn patch_resource<P: serde::Serialize, T: for<'a> serde::Deserialize<'a>>(
    app_address: &str,
    resource_name: &str,
    id: Uuid,
    patch: &P,
) -> Box<T> {
    patch_resource_response(app_address, resource_name, id, patch)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}