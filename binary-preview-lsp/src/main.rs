mod binary;
mod hover;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
