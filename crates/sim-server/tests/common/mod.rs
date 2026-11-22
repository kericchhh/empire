use std::net::SocketAddr;
use std::time::Duration;

use sim_server::game::{Config, Created, Game};
use sim_server::http::{HttpConfig, router};
use sim_server::store::Store;

const TINY: &str = include_str!("../../../sim-core/tests/fixtures/tiny.yaml");

/// The tiny world, served over HTTP on an ephemeral loopback port.
pub async fn serve(tick_deadline: Duration) -> (Created, SocketAddr) {
    let store = Store::in_memory().await.unwrap();
    let config = Config {
        tick_deadline,
        autoplay: false,
    };
    let created = Game::open(store, Some(TINY), 7, config).await.unwrap();
    let app = router(
        created.game.clone(),
        HttpConfig {
            admin_token: created.admin_token.clone(),
            allowed_hosts: vec![],
            max_wait: Duration::from_millis(200),
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (created, addr)
}
