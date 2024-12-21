use std::{fs::File, path::Path};

use anyhow::Context;
use console::Style;
use sqlx::SqlitePool;
use std::io::BufReader;
use tracing::{debug, info, trace};
use uuid::Uuid;

use crate::{
    pace::{instance_reader::PaceReader, Solution},
    utils::{
        directory::StrideDirectory,
        instance_data_db::InstanceDataDB,
        server_connection::ServerConnection,
        solution_upload::{is_score_good_enough_for_upload, SolutionUploadRequestBuilder},
        solver_executor::SolverResult,
        DId, IId,
    },
};

use super::arguments::{CommonOpts, ImportSolutionOpts};

#[derive(sqlx::FromRow, Debug)]
struct InstanceInfo {
    did: DId,
    best_score: Option<u32>,
    nodes: u32,
}

impl InstanceInfo {
    async fn read_for_instance(meta_db: &SqlitePool, iid: IId) -> anyhow::Result<Self> {
        sqlx::query_as::<_, InstanceInfo>(
            r"SELECT best_score, nodes, data_did as did FROM Instance WHERE iid = ?",
        )
        .bind(iid.iid_to_u32())
        .fetch_one(meta_db)
        .await
        .with_context(|| format!("Reading instance info for {iid:?}"))
    }
}

// TODO: de-duplicate this code
async fn open_db_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    if !path.is_file() {
        anyhow::bail!("Database file {path:?} does not exist. Run the >update< command first");
    }

    let pool = sqlx::sqlite::SqlitePool::connect(
        format!("sqlite:{}", path.to_str().expect("valid path name")).as_str(),
    )
    .await?;
    Ok(pool)
}

pub async fn command_import_solution(
    common_opts: &CommonOpts,
    cmd_opts: &ImportSolutionOpts,
) -> anyhow::Result<()> {
    let stride_dir = StrideDirectory::try_default()?;
    let meta_db = open_db_pool(stride_dir.db_meta_file().as_path()).await?;
    let instance_info = InstanceInfo::read_for_instance(&meta_db, cmd_opts.instance).await?;
    debug!("Read instance info: {:?}", instance_info);
    let server_conn = ServerConnection::new_from_opts(common_opts)?;

    // read in solution
    let solution = if let Some(path) = &cmd_opts.solution {
        trace!("Reading solution from file {:?}", path);
        let file = File::open(path)?;
        Solution::read(BufReader::new(file), Some(instance_info.nodes))
    } else {
        trace!("Reading solution from stdin");
        Solution::read(std::io::stdin().lock(), Some(instance_info.nodes))
    }
    .with_context(|| "Reading solution")?;

    info!("Read solution with cardinality {}", solution.solution.len());

    // verify solution
    {
        let instance_db = InstanceDataDB::new(stride_dir.db_instance_file().as_path()).await?;
        let data = instance_db
            .fetch_data_with_did(&server_conn, cmd_opts.instance, instance_info.did)
            .await?;
        let reader = PaceReader::try_new(data.as_bytes())
            .with_context(|| "Creating reader for instance data")?;
        let num_nodes = reader.number_of_nodes();
        let mut edges = Vec::with_capacity(reader.number_of_edges() as usize);
        for e in reader {
            edges.push(e.with_context(|| "Reading instance data")?);
        }
        trace!(
            "Read {num_nodes} nodes and {} edges from instance data",
            edges.len()
        );

        let is_valid = solution
            .valid_domset_for_instance(instance_info.nodes, edges.into_iter())
            .with_context(|| "Verifying solution")?;

        if !is_valid {
            anyhow::bail!("Solution is not valid for instance {:?}", cmd_opts.instance);
        }
    }
    println!(
        "The solution is {} for instance {} and has cardinality {}",
        Style::new().green().bold().apply_to("feasible"),
        cmd_opts.instance.iid_to_u32(),
        solution.solution.len(),
    );

    if !is_score_good_enough_for_upload(solution.solution.len() as u32, instance_info.best_score) {
        println!(
            "{}. Best known score: {}",
            Style::new()
                .yellow()
                .apply_to("Score is not good enough for upload"),
            instance_info.best_score.unwrap()
        );
        return Ok(());
    }

    // upload solution
    let result = SolverResult::Valid {
        data: solution.take_1indexed_solution(),
    };

    SolutionUploadRequestBuilder::default()
        .instance_id(cmd_opts.instance)
        .run_uuid(Uuid::new_v4())
        .solver_uuid(None)
        .result(&result)
        .build()
        .unwrap()
        .upload(&server_conn)
        .await?;

    println!("Upload complete");
    Ok(())
}
