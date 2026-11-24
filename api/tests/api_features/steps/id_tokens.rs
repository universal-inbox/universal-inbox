pub const FIRST_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTE5MDE1MjI5NzI1NDEyMSIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMjk5Nzc3LCJpYXQiOjE3MDAyNTY1NzcsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiI0bk1obE01bm5xbXFLcXJKcjVqTkd3IiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6InJxMl81N3dacjJqNmlLY1dvZzhDNkEiLCJjX2hhc2giOiJUbE5jLXJzLVlkN2dHaVIwNkRjcGpBIn0.qoOPG0_Ia40xq0jzlOeMUtrxK5LjZhQJS3_RfUbtRZxXEGWd8krreN7J3qmIKHo_Xp8Ih5BZJon1GqSYUkdqjcVg-a8XNXE-1kqAqz2ViPbDGtmSfx8tl7ga_cIH2hXsYy1zNMxtdmCbCFaKGUt6XOs201gcx-2kyJLMvN0mcZ23W6VxcVuo9_CR_BXWFjc9WVw-Ws34UhWOxk0_sNRwpTg720KHOcmxXH118dKGhWNpFG9qJYbDaXuBJ1jwS4RTMbC5cruXfQiNAJ0aaeZM52yIno16YSN44_cpllRQgzoNIXF2i8GS7c2M2D1mEssilTI55t2W4VihahmrCUScZg";
pub const SECOND_ID_TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjI0MTI3MDM2ODQ0NTIwNTczNyIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3Rlc3QteGJzYnMzLnppdGFkZWwuY2xvdWQiLCJzdWIiOiIxODE0MTE0MDYyODgwNjA2NzMiLCJhdWQiOlsiMjA1NjYyMjE0NDgzNDExMjAxQHVuaXZlcnNhbF9pbmJveCIsIjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCIyMDQzNTkzMDAyNTA4NjE4MjUiXSwiZXhwIjoxNzAwMzQ3NTUzLCJpYXQiOjE3MDAzMDQzNTMsImF1dGhfdGltZSI6MTY5NzcyMDU1Nywibm9uY2UiOiJ1R3NVdFNlWWVDS240dW4xdk9jZXRRIiwiYW1yIjpbInBhc3N3b3JkIiwicHdkIl0sImF6cCI6IjIwNDM1OTU2MDAyOTMzOTkwNUB1bml2ZXJzYWxfaW5ib3giLCJjbGllbnRfaWQiOiIyMDQzNTk1NjAwMjkzMzk5MDVAdW5pdmVyc2FsX2luYm94IiwiYXRfaGFzaCI6IkRIaG81UFJZbkJqajNUeDdHVXljR3ciLCJjX2hhc2giOiJ6WmxtcWFvQmMwQ1ZFSl83Z2ZjbFNRIn0.a4cJLj6Fx1c2wcKxoU_fqBtTtbLpjxOaU8NE9UhnGxts2G0iXjm6N6duXu2yRSaxWV8hRYuQ8PJrl--EAC4wGnQ7zC2AwGjay8zll2zQR3ErR6pghUaNu_7Xr7yXSvysSspsSFBvc5cPQ1EITngxOExydtybiF0AJldwiLTfM_lMK-TsD118yLdhvOsofyY3n8397HIv3xpZHJsoPgGdLmgnT57TJP7krpL8fomTUuAZIj_5txk426mq4b5WcQ5Sxk-MZ3Zt3ktmD7jP5qHU6Xw4uwY9kxxkGSQZnTeucds_OlOUcU7daig_sm3XegJH69khvcZTfcNmwbTCfuYgWA";

pub fn signed_id_token(issuer: String, email: &str) -> String {
    use chrono::{TimeDelta, Utc};
    use openidconnect::{
        Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, StandardClaims,
        SubjectIdentifier,
        core::{CoreHmacKey, CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm},
    };

    CoreIdToken::new(
        CoreIdTokenClaims::new(
            IssuerUrl::new(issuer).unwrap(),
            vec![Audience::new(format!("{email}-client-id-123"))],
            Utc::now() + TimeDelta::try_seconds(120).unwrap(),
            Utc::now(),
            StandardClaims::new(SubjectIdentifier::new("John-Doe".to_string()))
                .set_email(Some(EndUserEmail::new(email.to_string()))),
            EmptyAdditionalClaims {},
        ),
        &CoreHmacKey::new("secret".as_bytes()),
        CoreJwsSigningAlgorithm::HmacSha256,
        None,
        None,
    )
    .unwrap()
    .to_string()
}
