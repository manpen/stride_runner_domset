use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use priority_queue::PriorityQueue;
use stride_runner_domset::pace::{graph::*, instance_reader::PaceReader};
use structopt::StructOpt;

fn read_graph() -> anyhow::Result<Vec<Vec<Node>>> {
    let stdin = std::io::stdin().lock();
    let reader = PaceReader::try_new(stdin)?;

    let mut closed_neighbors = (0..reader.number_of_nodes())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<Node>>>();

    let number_of_edges_according_to_header = reader.number_of_edges();
    for edge in reader {
        let Edge(u, v) = edge?;
        closed_neighbors[u as usize].push(v);
        assert_ne!(u, v);
        closed_neighbors[v as usize].push(u);
    }

    debug_assert_eq!(
        closed_neighbors.iter().map(|nei| nei.len()).sum::<usize>(),
        2 * (number_of_edges_according_to_header as usize)
    );

    Ok(closed_neighbors)
}

fn greedy(graph: &[Vec<Node>]) -> Vec<Node> {
    let mut pq = PriorityQueue::new();

    for (node, neighbors) in graph.iter().enumerate() {
        pq.push(node as Node, neighbors.len() as Node);
    }

    let mut domset = Vec::new();

    while let Some((node, degree)) = pq.pop() {
        if degree == 0 {
            continue;
        }

        domset.push(node);
        for neighbor in &graph[node as usize] {
            pq.change_priority_by(neighbor, |d| *d -= 1);
        }
    }

    domset
}

fn print_result(opts: &Opt, domset: &[Node]) {
    if opts.add_comment {
        println!("c Greedy algorithm");
    }

    if opts.empty_lines {
        println!();
    }

    println!("{}", domset.len() + opts.wrong_cardinality as usize,);

    if opts.add_comment {
        println!("c Here goes another comment");
    }

    if opts.empty_lines {
        println!();
    }

    for u in domset {
        println!("{}", u + 1);
    }

    if opts.empty_lines {
        println!();
    }
}

#[derive(Debug, Clone, StructOpt)]
struct Opt {
    #[structopt(short = "-c", long)]
    add_comment: bool,

    #[structopt(short, long)]
    wrong_cardinality: bool,

    #[structopt(short, long)]
    infeasible: bool,

    #[structopt(
        short,
        long,
        help = "Sleep for this many seconds before printing",
        default_value = "0"
    )]
    sleep: u64,

    #[structopt(short = "-t", long)]
    wait_sigterm: bool,

    #[structopt(short, long)]
    never_terminate: bool,

    #[structopt(short, long)]
    empty_lines: bool,
}

fn wait_for_sigterm(opts: &Opt) -> anyhow::Result<()> {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;

    if opts.add_comment {
        println!("c Waiting for SIGTERM");
        std::io::stdout().flush()?;
    }

    while !term.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
    }

    if opts.add_comment {
        println!("c Got SIGTERM");
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let opts = Opt::from_args();

    let adj_list = read_graph()?;
    let mut domset = greedy(&adj_list);

    if opts.infeasible {
        domset.truncate(domset.len() / 2);
    }

    if opts.sleep > 0 {
        std::thread::sleep(Duration::from_secs(opts.sleep));
    }

    if opts.wait_sigterm {
        wait_for_sigterm(&opts)?;
    }

    if opts.never_terminate {
        // register handler such that SIGTERM gets ignored
        let term = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;

        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    print_result(&opts, &domset);

    Ok(())
}
