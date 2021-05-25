use warp::ws::{Message, WebSocket};
use warp::{Filter, http::Response};

use tokio::sync::broadcast::{Sender, Receiver};

use std::sync::{
    Arc, RwLock
};

use sqlx::Sqlite;
use sqlx::Pool;
use sqlx::Executor;

static index: &'static str = include_str!("../mock-frontend/index.html");
static js: &'static str = include_str!("../mock-frontend/main.js");
static css: &'static str = include_str!("../mock-frontend/main.css");

// ** Warp Routes **
// <static files>                -- The frontend resources, html, css, js, and woff2
//
// api/auth/authenticate {token} -- Attempts to use a token to auth the connection
// api/auth/login {user, pass}   -- Makes a login attempt, returns a new token if successful
// api/auth/register {id}        -- Accepts an invite
// api/invite {email}            -- Sends an invite request to the specified email
// invite/<id>                   -- External link for requested invite, handled client-side

pub async fn start_server(db: Arc<Pool<Sqlite>>, port: u16, mut shutdown_reader: Receiver<()>) {
    let index_route = warp::any().map(|| {
        warp::reply::html(index)
    });

    let js_route = warp::path!("main.js").map(|| {
        Response::builder()
            .header("content-type", "text/javascript; charset=utf-8")
            .body(js)
    });

    let css_route = warp::path!("main.css").map(|| {
        Response::builder()
            .header("content-type", "text/css; charset=utf-8")
            .body(css)
    });

    let routes = css_route.or(js_route).or(index_route);

    let (addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), async move {
            shutdown_reader.recv().await.ok();
        });

    server.await;
}
