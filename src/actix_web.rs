use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    web, App, Error, HttpResponse, Responder,
};

use crate::SerdeFunctionWrapper;

pub trait AppExt {
    fn register_server_fns(self) -> Self;
}

impl<T: ServiceFactory<ServiceRequest, Config = (), Error = Error, InitError = ()>> AppExt
    for App<T>
{
    fn register_server_fns(self) -> Self {
        let mut app = self;
        for server_fn in crate::server_fns() {
            let route = server_fn.path;
            let func = server_fn.func;
            app = app.route(&route, web::post().to(move |body| call_inner(body, func)));
        }
        app
    }
}

async fn call_inner(body: web::Bytes, func: SerdeFunctionWrapper) -> Option<impl Responder> {
    func(&body)
        .await
        .map(|bytes| HttpResponse::Ok().body(bytes))
        .ok()
}
