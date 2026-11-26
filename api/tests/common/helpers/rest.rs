use reqwest::{Client, Response};
use uuid::Uuid;

pub async fn get_resource_response(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Response {
    client
        .get(format!("{api_address}{resource_name}/{id}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn get_resource<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Box<T> {
    get_resource_response(client, api_address, resource_name, id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn create_resource_response<T: serde::Serialize>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    resource: Box<T>,
) -> Response {
    client
        .post(format!("{api_address}{resource_name}"))
        .json(&*resource)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn create_resource<T: serde::Serialize, U: for<'a> serde::Deserialize<'a>>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    resource: Box<T>,
) -> Box<U> {
    create_resource_response(client, api_address, resource_name, resource)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn patch_resource_response<P: serde::Serialize>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: Uuid,
    patch: &P,
) -> Response {
    client
        .patch(format!("{api_address}{resource_name}/{id}"))
        .json(patch)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn patch_resource<P: serde::Serialize, T: for<'a> serde::Deserialize<'a>>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: Uuid,
    patch: &P,
) -> Box<T> {
    patch_resource_response(client, api_address, resource_name, id, patch)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn delete_resource_response(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Response {
    client
        .delete(format!("{api_address}{resource_name}/{id}"))
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn delete_resource<T: serde::Serialize + for<'a> serde::Deserialize<'a>>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    id: uuid::Uuid,
) -> Box<T> {
    delete_resource_response(client, api_address, resource_name, id)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}

pub async fn patch_resource_collection_response<P: serde::Serialize>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    patch: &P,
) -> Response {
    client
        .patch(format!("{api_address}{resource_name}"))
        .json(patch)
        .send()
        .await
        .expect("Failed to execute request")
}

pub async fn patch_resource_collection<P: serde::Serialize, T: for<'a> serde::Deserialize<'a>>(
    client: &Client,
    api_address: &str,
    resource_name: &str,
    patch: &P,
) -> T {
    patch_resource_collection_response(client, api_address, resource_name, patch)
        .await
        .json()
        .await
        .expect("Cannot parse JSON result")
}
