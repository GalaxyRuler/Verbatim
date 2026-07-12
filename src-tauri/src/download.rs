use anyhow::{anyhow, Result};
use reqwest::{RequestBuilder, Response};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(crate) struct DownloadTimeouts {
    pub connect: Duration,
    pub read: Duration,
    pub total: Duration,
}

impl Default for DownloadTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(30),
            total: Duration::from_secs(30 * 60),
        }
    }
}

pub(crate) struct DownloadClient {
    client: reqwest::Client,
    timeouts: DownloadTimeouts,
}

impl DownloadClient {
    pub(crate) fn with_timeouts(timeouts: DownloadTimeouts) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.total)
            .build()?;
        Ok(Self { client, timeouts })
    }

    pub(crate) fn get(&self, url: &str) -> RequestBuilder {
        self.client.get(url)
    }

    pub(crate) fn total_timeout(&self) -> Duration {
        self.timeouts.total
    }

    pub(crate) async fn send(
        &self,
        request: RequestBuilder,
        cancelled: &AtomicBool,
    ) -> Result<Response> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow!("download cancelled before request"));
        }

        let request = request.send();
        tokio::pin!(request);
        let deadline = Instant::now() + self.timeouts.total;
        let mut cancellation_poll = tokio::time::interval(Duration::from_millis(20));

        loop {
            if Instant::now() >= deadline {
                return Err(anyhow!("download request timed out"));
            }

            tokio::select! {
                response = &mut request => {
                    return response.map_err(|error| anyhow!("download request failed: {error:?}"));
                }
                _ = cancellation_poll.tick() => {
                    if cancelled.load(Ordering::Relaxed) {
                        return Err(anyhow!("download cancelled during request"));
                    }
                }
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                    return Err(anyhow!("download request timed out"));
                }
            }
        }
    }

    pub(crate) async fn next_chunk(
        &self,
        response: &mut Response,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow!("download cancelled while reading"));
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!("download exceeded total deadline"));
        }

        let read_deadline = self.timeouts.read.min(remaining);
        let mut cancellation_poll = tokio::time::interval(Duration::from_millis(20));
        tokio::time::timeout(read_deadline, async {
            loop {
                tokio::select! {
                    chunk = response.chunk() => {
                        return chunk
                            .map(|chunk| chunk.map(|chunk| chunk.to_vec()))
                            .map_err(|error| anyhow!("download body read failed: {error}"));
                    }
                    _ = cancellation_poll.tick() => {
                        if cancelled.load(Ordering::Relaxed) {
                            return Err(anyhow!("download cancelled while reading"));
                        }
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow!("download body read timed out"))?
    }
}

impl Default for DownloadClient {
    fn default() -> Self {
        Self::with_timeouts(DownloadTimeouts::default())
            .expect("default download client configuration must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn stalled_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: keep-alive\r\n\r\nx",
                )
                .unwrap();
            thread::sleep(Duration::from_secs(2));
        });
        format!("http://{address}/stalled")
    }

    #[tokio::test]
    async fn stalled_body_aborts_when_cancelled() {
        let client = DownloadClient::with_timeouts(DownloadTimeouts {
            connect: Duration::from_millis(100),
            read: Duration::from_secs(1),
            total: Duration::from_secs(5),
        })
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let url = stalled_server();
        let response = client.send(client.get(&url), &cancelled).await.unwrap();
        let mut response = response;
        cancelled.store(true, Ordering::Relaxed);

        let started = Instant::now();
        let error = client
            .next_chunk(
                &mut response,
                &cancelled,
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn stalled_body_aborts_on_read_timeout() {
        let client = DownloadClient::with_timeouts(DownloadTimeouts {
            connect: Duration::from_millis(100),
            read: Duration::from_millis(100),
            total: Duration::from_secs(5),
        })
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let url = stalled_server();
        let response = client.send(client.get(&url), &cancelled).await.unwrap();
        let mut response = response;

        assert_eq!(
            client
                .next_chunk(
                    &mut response,
                    &cancelled,
                    Instant::now() + Duration::from_secs(5)
                )
                .await
                .unwrap(),
            Some(vec![b'x'])
        );

        let error = client
            .next_chunk(
                &mut response,
                &cancelled,
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }
}
