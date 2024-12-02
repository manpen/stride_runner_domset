use std::path::{Path, PathBuf};

const PATH_CONFIG: &str = "config.json";
const PATH_DB_META: &str = "metadata.db";
const PATH_DB_CACHE: &str = "cache.db";
const PATH_DB_INSTANCES: &str = "instances.db";

const DATA_DIR: &str = ".stride";

pub struct StrideDirectory {
    pub data_dir: PathBuf,
}

impl StrideDirectory {
    /// Create a new StrideDirectory instance and ensures that
    ///  - the data directory exists
    ///  - the data directory is a directory
    pub fn try_new(data_dir: PathBuf) -> anyhow::Result<Self> {
        if data_dir.exists() {
            if !data_dir.is_dir() {
                anyhow::bail!("Data directory is not a directory");
            }
        } else {
            std::fs::create_dir_all(&data_dir)?;
        }

        Ok(Self { data_dir })
    }

    pub fn try_default() -> anyhow::Result<Self> {
        Self::try_new(PathBuf::from(DATA_DIR))
    }

    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_path()
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join(PATH_CONFIG)
    }

    pub fn db_meta_file(&self) -> PathBuf {
        self.data_dir.join(PATH_DB_META)
    }

    pub fn db_cache_file(&self) -> PathBuf {
        self.data_dir.join(PATH_DB_CACHE)
    }

    pub fn db_instance_file(&self) -> PathBuf {
        self.data_dir.join(PATH_DB_INSTANCES)
    }
}

#[cfg(test)]
mod test {
    use tempdir::TempDir;

    const PREFIX: &str = "stride-dir-test";
    const DATA_DIR: &str = ".stride";

    #[test]
    fn try_new_not_existing() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let data_dir = tmp_dir.path().join(DATA_DIR);
        let stride_dir = super::StrideDirectory::try_new(data_dir.clone());
        assert!(stride_dir.is_ok());
        assert!(data_dir.exists());
    }

    #[test]
    fn try_new_existing() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let data_dir = tmp_dir.path().join(DATA_DIR);
        std::fs::create_dir_all(&data_dir).unwrap();
        let stride_dir = super::StrideDirectory::try_new(data_dir.clone());
        assert!(stride_dir.is_ok());
        assert!(data_dir.exists());
    }

    #[test]
    fn try_new_not_dir() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let data_dir = tmp_dir.path().join(DATA_DIR);
        std::fs::File::create(&data_dir).unwrap();
        let stride_dir = super::StrideDirectory::try_new(data_dir.clone());
        assert!(stride_dir.is_err());
    }

    macro_rules! check_filename {
        ($name:ident, $ref:ident) => {
            #[test]
            fn $name() {
                let tmp_dir = TempDir::new(PREFIX).unwrap();
                let data_dir = tmp_dir.path().join(DATA_DIR);
                std::fs::create_dir_all(&data_dir).unwrap();
                let stride_dir = super::StrideDirectory::try_new(data_dir.clone()).unwrap();

                let config_file = stride_dir.$name();
                assert_eq!(config_file.file_name().unwrap(), super::$ref);
                assert_eq!(config_file.parent().unwrap(), data_dir.as_path());
            }
        };
    }

    check_filename!(config_file, PATH_CONFIG);
    check_filename!(db_meta_file, PATH_DB_META);
    check_filename!(db_cache_file, PATH_DB_CACHE);
    check_filename!(db_instance_file, PATH_DB_INSTANCES);
}
