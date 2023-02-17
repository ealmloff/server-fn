use axum::{body::Bytes, routing::post, Router};

use crate::SerdeFunctionWrapper;

pub trait RouterExt {
    fn register_server_fns(self) -> Self;
}

impl RouterExt for Router {
    fn register_server_fns(self) -> Self {
        let mut router = self;
        for server_fn in crate::server_fns() {
            let route = server_fn.path;
            let func = server_fn.func;
            println!("Registering route: {:?}", server_fn);
            router = router.route(&route, post(move |body| call_inner(body, func)));
        }
        router
    }
}

async fn call_inner(body: Bytes, func: SerdeFunctionWrapper) -> Result<Vec<u8>, String> {
    func(&body).await.map_err(|err| {
        log::error!("Error calling server function: {:?}", err);
        format!("{:?}", err)
    })
}
