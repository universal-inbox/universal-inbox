use anyhow::anyhow;
use graphql_client::Response;

use crate::universal_inbox::UniversalInboxError;

pub fn assert_no_error_in_graphql_response<T>(
    response: &Response<T>,
    graphql_api_name: &str,
) -> Result<(), UniversalInboxError> {
    if let Some(ref errors) = response.errors
        && !errors.is_empty()
    {
        let error_messages = errors
            .iter()
            .map(|error| error.message.clone())
            .collect::<Vec<String>>()
            .join(", ");
        // If there is a well formated error, we can assume that the error is not transient
        // and we should not retry the request.
        return Err(UniversalInboxError::Recoverable(anyhow!(
            "Errors occured while querying {} API: {}",
            graphql_api_name,
            error_messages
        )));
    }

    Ok(())
}
