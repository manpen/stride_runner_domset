use std::fs::File;

use anyhow::Context;
use console::Style;
use std::io::BufReader;
use tracing::{debug, info, trace};
use uuid::Uuid;

use crate::{
    pace::{instance_reader::PaceReader, Solution},
    utils::{
        directory::StrideDirectory,
        instance_data_db::InstanceDataDB,
        meta_data_db::MetaDataDB,
        server_connection::ServerConnection,
        solution_upload::{is_score_good_enough_for_upload, SolutionUploadRequestBuilder},
        solver_executor::SolverResult,
    },
};

use super::arguments::{CommonOpts, ImportSolutionOpts};

pub async fn command_import_solution(
    common_opts: &CommonOpts,
    cmd_opts: &ImportSolutionOpts,
) -> anyhow::Result<()> {
    let stride_dir = StrideDirectory::try_default()?;
    let meta_db = MetaDataDB::new(stride_dir.db_meta_file().as_path()).await?;
    let instance_info = meta_db.fetch_instance(cmd_opts.instance).await?;
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
            .fetch_data_with_did(&server_conn, cmd_opts.instance, instance_info.data_did)
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
