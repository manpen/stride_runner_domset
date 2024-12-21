use anyhow::Context;
use sqlx::SqlitePool;
use std::path::Path;
use tracing::trace;

use super::{DId, IId};

pub struct MetaDataDB {
    meta_db: SqlitePool,
}

#[derive(sqlx::FromRow, Debug)]
pub struct InstanceModel {
    pub iid: IId,
    pub data_did: DId,
    pub nodes: u32,
    pub edges: u32,
    pub best_score: Option<u32>,
    pub diameter: Option<u32>,
    pub treewidth: Option<u32>,
    pub planar: Option<bool>,
    pub bipartite: Option<bool>,
}

pub struct DangerousRawClause<'a>(pub &'a str);

impl MetaDataDB {
    pub async fn new(db_path: &Path) -> anyhow::Result<Self> {
        let meta_db = Self::open_db_pool(db_path).await?;
        Ok(Self { meta_db })
    }

    pub async fn fetch_did_of_iid(&self, iid: IId) -> anyhow::Result<DId> {
        trace!("Starting fetch_did_of_iid");
        sqlx::query_scalar::<_, DId>("SELECT data_did FROM Instance WHERE iid = ? LIMIT 1")
            .bind(iid.iid_to_u32())
            .fetch_one(&self.meta_db)
            .await
            .with_context(|| format!("Fetching data_did for iid {iid:?}"))
    }

    pub async fn fetch_instance(&self, iid: IId) -> anyhow::Result<InstanceModel> {
        trace!("Starting fetch_instance");
        sqlx::query_as::<_, InstanceModel>(
            r"SELECT iid, data_did, nodes, edges, best_score, diameter, treewidth, planar, bipartite FROM Instance WHERE iid = ?",
        )
        .bind(iid.iid_to_u32())
        .fetch_one(&self.meta_db)
        .await
        .with_context(|| format!("Fetching instance info for {iid:?}"))
    }

    /// there might be some "security" implications here, but I do not really care:
    /// the sqlite database is fully under user control and worst-case the
    /// user needs to re-pull it after they (intentionally) messed it up ...
    pub async fn fetch_instance_iids_from_db(
        &self,
        DangerousRawClause(where_clause): DangerousRawClause<'_>,
    ) -> anyhow::Result<Vec<IId>> {
        trace!("Starting fetch_instance_iids_from_db");
        let instances = sqlx::query_scalar::<_, IId>(&format!(
            "SELECT iid FROM Instance WHERE {}",
            where_clause
        ))
        .fetch_all(&self.meta_db)
        .await?;

        Ok(instances)
    }

    async fn open_db_pool(path: &Path) -> anyhow::Result<SqlitePool> {
        trace!("Starting open_db_pool");
        if !path.is_file() {
            anyhow::bail!("Database file {path:?} does not exist. Run the >update< command first");
        }

        let pool = sqlx::sqlite::SqlitePool::connect(
            format!("sqlite:{}", path.to_str().expect("valid path name")).as_str(),
        )
        .await?;
        Ok(pool)
    }
}
