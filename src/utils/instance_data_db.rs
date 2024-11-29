use log::debug;
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePoolOptions, Sqlite, SqlitePool};
use std::path::Path;

use super::server_connection::ServerConnection;

pub struct InstanceDataDB {
    db: SqlitePool,
}

impl InstanceDataDB {
    pub async fn new(db_path: &Path) -> anyhow::Result<Self> {
        let db = Self::connect_or_create_db(db_path).await?;
        Ok(Self { db })
    }

    pub async fn fetch_data(
        &self,
        server_conn: &ServerConnection,
        iid: u32,
    ) -> anyhow::Result<Option<String>> {
        let from_db = self.fetch_data_from_db(iid).await?;
        if let Some(data) = from_db {
            return Ok(Some(data));
        }

        let from_server = self.fetch_from_server(server_conn, iid).await?;
        // TODO: We may need to handle locks here
        self.insert_into_db(iid, &from_server).await?;
        Ok(Some(from_server))
    }

    async fn connect_or_create_db(path: &Path) -> anyhow::Result<SqlitePool> {
        let path = match path.to_str() {
            Some(path) => path,
            None => anyhow::bail!("Path is not valid utf-8"),
        };
        let already_exists = Sqlite::database_exists(path).await?;

        if !already_exists {
            debug!("Creating database {}", path);
            Sqlite::create_database(path).await?;
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(path)
            .await?;

        debug!("Connection to SQLite database {path} is successful!");

        if !already_exists {
            sqlx::query("CREATE TABLE InstanceData ( did INT PRIMARY KEY, data LONGBLOB);")
                .execute(&pool)
                .await
                .expect("Failed to create SQLite tables");
        }

        Ok(pool)
    }

    async fn fetch_data_from_db(&self, iid: u32) -> anyhow::Result<Option<String>> {
        match sqlx::query_scalar::<_, String>("SELECT data FROM InstanceData WHERE did = ? LIMIT 1")
            .bind(iid)
            .fetch_one(&self.db)
            .await
        {
            Ok(data) => Ok(Some(data)),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn insert_into_db(&self, iid: u32, data: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO InstanceData (did, data) VALUES (?, ?);")
            .bind(iid)
            .bind(data)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    pub async fn fetch_from_server(
        &self,
        server_conn: &ServerConnection,
        iid: u32,
    ) -> anyhow::Result<String> {
        let url = server_conn
            .base_url()
            .join(&format!("api/instances/download/{}", iid))?;

        let resp = server_conn.client_arc().get(url).send().await?;
        resp.error_for_status_ref()?;

        Ok(resp.text().await?)
    }
}

#[cfg(test)]
mod test {
    use reqwest::Url;
    use tempdir::TempDir;

    use crate::{
        pace::instance_reader::PaceReader,
        utils::{instance_data_db::InstanceDataDB, server_connection::ServerConnection},
    };

    const PREFIX: &str = "stride-instance-data-db-test";
    const SERVER: &str = "https://domset.algorithm.engineering";

    const REF_ID: u32 = 1582;
    const REF_DATA: &str = "p ds 9 8\n1 3\n1 4\n1 7\n2 8\n3 9\n4 8\n4 9\n5 6\n";

    #[tokio::test]
    async fn create_and_connect() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");

        // the first call will create the db
        {
            let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();
            db.insert_into_db(1, "Hello").await.unwrap();
        }

        // the second should reconnect to the existing db
        {
            let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();
            db.insert_into_db(2, "Hi").await.unwrap();

            // this entry we previously inserted should still be there
            assert!(db.insert_into_db(1, "Hello").await.is_err());
        }
    }

    #[tokio::test]
    async fn fetch_data() {
        const ID: u32 = 1;
        const VALUE: &str = "Hello";

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        // fetch existing row
        {
            db.insert_into_db(ID, VALUE).await.unwrap();
            let data = db.fetch_data_from_db(ID).await.unwrap();
            assert_eq!(data, Some(VALUE.to_string()));
        }

        // fetch non-existing row
        assert!(db
            .fetch_data_from_db(ID + 1)
            .await
            .is_ok_and(|x| x.is_none()))
    }

    fn assert_data_matches_ref(data: &str) {
        let mut reader = PaceReader::try_new(data.as_bytes()).unwrap();
        let mut ref_reader = PaceReader::try_new(REF_DATA.as_bytes()).unwrap();

        assert_eq!(reader.number_of_nodes(), ref_reader.number_of_nodes());
        assert_eq!(reader.number_of_edges(), ref_reader.number_of_edges());

        while let Some(Ok(ref_edge)) = ref_reader.next() {
            let edge = reader.next().unwrap().unwrap();
            assert_eq!(edge, ref_edge);
        }
    }

    #[tokio::test]
    async fn fetch_from_server() {
        let server_conn = ServerConnection::new(Url::parse(SERVER).unwrap()).unwrap();

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        let data = db.fetch_from_server(&server_conn, REF_ID).await.unwrap();
        assert_data_matches_ref(&data);
    }

    #[tokio::test]
    async fn fetch_data_from_db_or_server() {
        let server_conn = ServerConnection::new(Url::parse(SERVER).unwrap()).unwrap();

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        // fetch from server
        {
            let data = db.fetch_data(&server_conn, REF_ID).await.unwrap().unwrap();
            assert_data_matches_ref(&data);
        }

        // fetch from db
        {
            let data = db.fetch_data(&server_conn, REF_ID).await.unwrap().unwrap();
            assert_data_matches_ref(&data);
        }
    }
}
