use anyhow::Context;
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePoolOptions, Sqlite, SqlitePool};
use std::path::Path;
use tracing::{debug, trace};

use super::server_connection::ServerConnection;
use super::*;

pub struct InstanceDataDB {
    instance_data_db: SqlitePool,
}

impl InstanceDataDB {
    pub async fn new(db_path: &Path) -> anyhow::Result<Self> {
        let db = Self::connect_or_create_db(db_path).await?;
        Ok(Self {
            instance_data_db: db,
        })
    }

    pub async fn fetch_data(
        &self,
        server_conn: &ServerConnection,
        meta_db: &SqlitePool,
        iid: IId,
    ) -> anyhow::Result<String> {
        let did = self
            .get_did_from_iid(meta_db, iid)
            .await
            .with_context(|| format!("Fetching DID for {:?}", iid))?;

        self.fetch_data_with_did(server_conn, iid, did).await
    }

    pub async fn fetch_data_with_did(
        &self,
        server_conn: &ServerConnection,
        iid: IId,
        did: DId,
    ) -> anyhow::Result<String> {
        let from_db = self.fetch_data_from_db(did).await?;
        if let Some(data) = from_db {
            debug!(
                "Fetched instance data for {iid:?} / {did:?} from db; size: {}",
                data.len()
            );
            return Ok(data);
        }

        let from_server = self.fetch_from_server(server_conn, iid).await?;

        debug!(
            "Fetched data for {iid:?} from server; size: {}",
            from_server.len()
        );

        // TODO: We may need to handle locks here
        self.insert_into_db(did, &from_server).await?;
        Ok(from_server)
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

        trace!("Connection to SQLite database {path} is successful!");

        if !already_exists {
            debug!("Creating table `InstanceData` in database {}", path);
            sqlx::query("CREATE TABLE InstanceData ( did INT PRIMARY KEY, data LONGBLOB);")
                .execute(&pool)
                .await
                .expect("Failed to create SQLite tables");
        }

        Ok(pool)
    }

    async fn fetch_data_from_db(&self, did: DId) -> anyhow::Result<Option<String>> {
        match sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT data FROM InstanceData WHERE did = ? LIMIT 1",
        )
        .bind(did.0)
        .fetch_one(&self.instance_data_db)
        .await
        {
            Ok(data) => Ok(Some(String::from_utf8(data)?)),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_did_from_iid(&self, meta_db: &SqlitePool, iid: IId) -> anyhow::Result<DId> {
        match sqlx::query_scalar::<_, u32>("SELECT data_did FROM Instance WHERE iid = ? LIMIT 1")
            .bind(iid.0)
            .fetch_one(meta_db)
            .await
        {
            Ok(did) => Ok(DId(did)),
            Err(e) => Err(e.into()),
        }
    }

    async fn insert_into_db(&self, did: DId, data: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO InstanceData (did, data) VALUES (?, ?);")
            .bind(did.0)
            .bind(data)
            .execute(&self.instance_data_db)
            .await?;

        Ok(())
    }

    pub async fn fetch_from_server(
        &self,
        server_conn: &ServerConnection,
        iid: IId,
    ) -> anyhow::Result<String> {
        let url = server_conn
            .base_url()
            .join(&format!("api/instances/download/{}", iid.0))?;

        let resp = server_conn.client_arc().get(url).send().await?;
        resp.error_for_status_ref()?;

        Ok(resp.text().await?)
    }

    pub async fn add_from_db_file(&self, other: &Path) -> anyhow::Result<()> {
        let path = match other.to_str() {
            Some(path) => path,
            None => anyhow::bail!("Path is not valid utf-8"),
        };

        sqlx::query("ATTACH ? AS download")
            .bind(path)
            .execute(&self.instance_data_db)
            .await?;

        sqlx::query("INSERT OR IGNORE INTO InstanceData (did, data) SELECT did, data FROM download.InstanceData").execute(&self.instance_data_db).await?;

        sqlx::query("DETACH download")
            .execute(&self.instance_data_db)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tempdir::TempDir;

    use crate::{
        pace::instance_reader::PaceReader,
        utils::{instance_data_db::InstanceDataDB, server_connection::ServerConnection},
    };

    use super::*;

    const PREFIX: &str = "stride-instance-data-db-test";

    const REF_IID: IId = IId(1582);
    const REF_DID: DId = DId(1670);
    const REF_DATA: &str = "p ds 9 8\n1 3\n1 4\n1 7\n2 8\n3 9\n4 8\n4 9\n5 6\n";

    #[tokio::test]
    async fn create_and_connect() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");

        // the first call will create the db
        {
            let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();
            db.insert_into_db(DId(1), "Hello").await.unwrap();
        }

        // the second should reconnect to the existing db
        {
            let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();
            db.insert_into_db(DId(2), "Hi").await.unwrap();

            // this entry we previously inserted should still be there
            assert!(db.insert_into_db(DId(1), "Hello").await.is_err());
        }
    }

    #[tokio::test]
    async fn fetch_data() {
        const DID: DId = DId(1);
        const VALUE: &str = "Hello";

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");

        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        // fetch existing row
        {
            db.insert_into_db(DID, VALUE).await.unwrap();
            let data = db.fetch_data_from_db(DID).await.unwrap();
            assert_eq!(data, Some(VALUE.to_string()));
        }

        // fetch non-existing row
        assert!(db
            .fetch_data_from_db(DId(DID.0 + 1))
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
        let server_conn = ServerConnection::try_default().unwrap();

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        let data = db.fetch_from_server(&server_conn, REF_IID).await.unwrap();
        assert_data_matches_ref(&data);
    }

    #[tokio::test]
    async fn fetch_data_from_db_or_server() {
        let server_conn = ServerConnection::try_default().unwrap();

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let db = InstanceDataDB::new(db_path.as_path()).await.unwrap();

        // fetch from server
        {
            let data = db
                .fetch_data_with_did(&server_conn, REF_IID, REF_DID)
                .await
                .unwrap();
            assert_data_matches_ref(&data);
        }

        // fetch from db
        {
            let data = db
                .fetch_data_with_did(&server_conn, REF_IID, REF_DID)
                .await
                .unwrap();
            assert_data_matches_ref(&data);
        }
    }
}
