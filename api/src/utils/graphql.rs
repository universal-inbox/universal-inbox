use anyhow::anyhow;
use graphql_client::Response;

use crate::universal_inbox::UniversalInboxError;

pub fn assert_no_error_in_graphql_response<T>(
    response: &Response<T>,
    graphql_api_name: &str,
) -> Result<(), UniversalInboxError> {
    if let Some(ref errors) = response.errors {
        if !errors.is_empty() {
            let error_messages = errors
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<String>>()
                .join(", ");
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Errors occured while querying {} API: {}",
                graphql_api_name,
                error_messages
            )));
        }
    }

    Ok(())
}
