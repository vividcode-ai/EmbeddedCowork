use std::sync::Arc;

use axum::body::Body;
use axum::response::Response;
use http::StatusCode;

/// Shared HTTP proxy client for forwarding requests to backend services.
#[derive(Clone)]
pub struct ProxyClient {
    client: reqwest::Client,
}

impl ProxyClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("Failed to create proxy HTTP client"),
        }
    }

    pub async fn forward_request(
        &self,
        method: reqwest::Method,
        target_url: &str,
        headers: &http::HeaderMap,
        body: Vec<u8>,
    ) -> Result<Response<Body>, StatusCode> {
        let mut req = self.client.request(method, target_url);

        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }

        if !body.is_empty() {
            req = req.body(body);
        }

        let resp = req.send().await.map_err(|e| {
            tracing::warn!(target = %target_url, error = %e, "Proxy request failed");
            StatusCode::BAD_GATEWAY
        })?;

        let status = resp.status();
        let resp_headers = resp.headers().clone();
        let resp_body = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

        let body_str = String::from_utf8_lossy(&resp_body);
        println!(
            "[PROXY] <-- {} {} body={}",
            target_url,
            status.as_u16(),
            body_str,
        );
        println!(
            "[PROXY] <-- {} response headers: {:?}",
            target_url,
            resp_headers,
        );

        let mut builder = Response::builder().status(status);
        for (key, value) in resp_headers.iter() {
            let name = key.as_str().to_lowercase();
            if name != "transfer-encoding" && name != "content-length" {
                builder = builder.header(key, value);
            }
        }

        builder.body(Body::from(resp_body)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}

pub type SharedProxyClient = Arc<ProxyClient>;

pub fn create_proxy_client() -> SharedProxyClient {
    Arc::new(ProxyClient::new())
}
