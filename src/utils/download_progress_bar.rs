use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::server_connection::{DownloadProgress, DownloadProgressCallback};

pub struct DownloadProgressBar {
    pb: ProgressBar,
}

impl DownloadProgressBar {
    pub fn new(parent: &MultiProgress, name: String) -> anyhow::Result<Self> {
        let pb = parent.add(ProgressBar::no_length());
        pb.set_style(ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
        .progress_chars("#>-"));

        pb.set_message(name);

        Ok(Self { pb })
    }
}

impl DownloadProgressCallback for DownloadProgressBar {
    fn init(&mut self, total_size: Option<u64>) {
        if let Some(size) = total_size {
            self.pb.set_length(size);
        }
    }

    fn update(&mut self, state: DownloadProgress) {
        self.pb.set_position(state.downloaded);
    }
}
