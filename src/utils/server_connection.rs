use futures_util::StreamExt;
use reqwest::{Client, ClientBuilder, Url};
use std::sync::Arc;
use std::{cmp::min, fs::File, io::Write, path::Path, time::Instant};
use tracing::debug;
use uuid::Uuid;

use crate::commands::arguments::CommonOpts;

pub const DEFAULT_SERVER_URL: &str = "https://domset.algorithm.engineering";

pub struct ServerConnection {
    client: Arc<Client>,
    base_url: Url,
}

pub struct DownloadProgress {
    pub started: Instant,
    pub total_size: Option<u64>,
    pub downloaded: u64,
}

pub trait DownloadProgressCallback {
    fn init(&mut self, _total_size: Option<u64>) {}
    fn update(&mut self, _state: DownloadProgress) {}
    fn done(&mut self) {}
}

struct NoOpCallaback();
impl DownloadProgressCallback for NoOpCallaback {}

// TODO: Client is internally a Arc; remove the superfluous external

impl ServerConnection {
    pub fn try_default() -> anyhow::Result<Self> {
        Self::new(Url::parse(DEFAULT_SERVER_URL).unwrap())
    }

    pub fn new_from_opts(opts: &CommonOpts) -> anyhow::Result<Self> {
        Self::new(opts.server_url().clone())
    }

    pub fn new(base_url: Url) -> anyhow::Result<Self> {
        let client = Arc::new(
            ClientBuilder::new()
                .danger_accept_invalid_certs(true)
                .build()?,
        );

        Ok(Self { client, base_url })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn solver_website_for_user(&self, uuid: Uuid) -> Url {
        let path = format!("runs.html?solver={}", uuid);
        self.base_url.join(&path).unwrap()
    }

    pub fn client_arc(&self) -> Arc<Client> {
        self.client.clone()
    }

    pub async fn download_file_with_updates<C: DownloadProgressCallback>(
        &self,
        url_without_host: &str,
        to_path: &Path,
        callback: &mut C,
    ) -> anyhow::Result<()> {
        let from_url = self.base_url.join(url_without_host)?;
        debug!("Downloading {} to {:?}", from_url, to_path);

        let res = self.client.get(from_url.as_str()).send().await?;
        res.error_for_status_ref()?;
        let total_size = res.content_length();

        callback.init(total_size);

        let mut stream = res.bytes_stream();

        let mut file = File::create(to_path)?;

        let mut downloaded: u64 = 0;
        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            if let Some(total_size) = total_size {
                downloaded = min(total_size, downloaded);
            }

            callback.update(DownloadProgress {
                started: Instant::now(),
                total_size,
                downloaded,
            });
        }

        debug!("Download {} to {:?} DONE", from_url, to_path);
        callback.done();

        Ok(())
    }

    pub async fn download_file(
        &self,
        url_without_host: &str,
        to_path: &Path,
    ) -> anyhow::Result<()> {
        self.download_file_with_updates(url_without_host, to_path, &mut NoOpCallaback())
            .await
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_server_url_is_valid() {
        Url::parse(super::DEFAULT_SERVER_URL).unwrap();
    }
}
