use anyhow::Context;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use tokio::sync::Mutex;
use tracing::trace;

use super::{DId, IId};

pub struct MetaDataDB {
    meta_db: Mutex<Connection>,
}

#[derive(Clone, Debug)]
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
        Ok(Self {
            meta_db: Mutex::new(meta_db),
        })
    }

    pub async fn fetch_did_of_iid(&self, iid: IId) -> anyhow::Result<DId> {
        trace!("Starting fetch_did_of_iid");

        let conn = self.meta_db.lock().await;

        let mut stmt = conn.prepare("SELECT data_did FROM Instance WHERE iid = ?1 LIMIT 1")?;
        stmt.query_row([iid.iid_to_u32()], |row| Ok(DId::new(row.get(0)?)))
            .with_context(|| format!("Fetching data_did for iid {iid:?}"))
    }

    pub async fn fetch_instance(&self, iid: IId) -> anyhow::Result<InstanceModel> {
        trace!("Starting fetch_instance");

        let conn = self.meta_db.lock().await;
        let mut stmt = conn.prepare_cached(
            r"SELECT iid, data_did, nodes, edges, best_score, diameter, treewidth, planar, bipartite FROM Instance WHERE iid = ?1",
        )?;

        stmt.query_row([iid.iid_to_u32()], |row| {
            Ok(InstanceModel {
                iid: IId::new(row.get(0)?),
                data_did: DId::new(row.get(1)?),
                nodes: row.get(2)?,
                edges: row.get(3)?,
                best_score: row.get(4)?,
                diameter: row.get(5)?,
                treewidth: row.get(6)?,
                planar: row.get(7)?,
                bipartite: row.get(8)?,
            })
        })
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

        let sql = format!("SELECT iid FROM Instance WHERE {}", where_clause);

        let conn = self.meta_db.lock().await;
        let mut stmt = conn
            .prepare_cached(&sql)
            .with_context(|| format!("Preparing statement for {sql}"))?;

        let mut rows = stmt.query([])?;

        let mut iids = Vec::new();
        while let Some(row) = rows.next()? {
            iids.push(IId::new(row.get(0)?));
        }

        Ok(iids)
    }

    async fn open_db_pool(path: &Path) -> anyhow::Result<Connection> {
        trace!("Starting open_db_pool");
        if !path.is_file() {
            anyhow::bail!("Database file {path:?} does not exist. Run the >update< command first");
        }

        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("Opening database {path:?}"))
    }
}
