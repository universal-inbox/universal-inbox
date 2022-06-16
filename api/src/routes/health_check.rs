use actix_web::HttpResponse;

pub async fn ping() -> HttpResponse {
    HttpResponse::Ok().finish()
}
