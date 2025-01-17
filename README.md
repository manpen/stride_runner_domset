# STRIDE [![Rust](https://github.com/manpen/stride_runner_domset/actions/workflows/rust.yml/badge.svg)](https://github.com/manpen/stride_runner_domset/actions/workflows/rust.yml)
see also:
 - [STRIDE website](https://domset.algorithm.engineering)
 - [Source code of the instance server](https://github.com/manpen/stride_server_domset)
 - [Getting Started on YouTube](https://youtu.be/ip8TgMZ6m2Y)

The STRIDE system is designed as an *unofficial* companion to the [PACE 2025 Dominating Set challenge](https://pacechallenge.org/2025/ds/).
It provides a [large database of problem instances](https://domset.algorithm.engineering) and solutions.
We hope that this will help to produce more general solvers that have fewer bugs.

Since PACE is a competition, all solutions are kept secret until the end of the PACE challenge.
Until then, only the Dominating Set Cardinalities (scores) of the best solution sofar is made available. 
Since solutions are verified upon upload on the server, the published scores have been certified.

After the challenge is completed in June 2025, we will publish the solutions under a free license.
We hope to provide an interesting dataset (e.g., for machine learning) that way.
**The whole system is designed as a community effort. 
If you are using the instances, we kindly ask you to share your solutions, especially if they are better than the ones we know so far.**
The STRIDE Runner makes that pretty easy.
Thank you!!!

## The STRIDE Runner
![Screencast: Installation & Execution of Runner](docs/runner.gif)

This small tool allows you to run your solver on a predefined sets of instances:
- Solvers can be started in parallel
- Downloading and caching of instances is done automatically
- You can define timeouts (until SIGTERM) and grace periods (until SIGKILL) to stop the solver
- Solutions are verified
- Solutions near or better than the best known solution are automatically uploaded to the server (can be disabled, but please don't 🙏)

### Additional Opt-In: Tracking your solution
![Screenshot: Overview of Runs](docs/web-runs.png)

As an added bonus you can anonymously "register" (see below) your solver --- which simply assigns a random UUID (v4) to your uploads.
In this mode, the runner will link all your uploads to this UUID and also upload some metadata (runtime, score, validity of solution) for failed runs.
You can then visualize and track the performance of your via our website.

## Getting the Runner
For technical reasons (see below), we currently only support Unix-based operating systems:

| OS      | Support      | Interactive Tests | Tests in CI |
| ------- | ------------ | ----------------- | ----------- |
| Linux   | best         | on all commits    | yes         |
| OSX     | good         | sometimes         | yes         |
| Windows | only in WSL* | none              | no          |

*Since the runner relies on unix signals (SIGTERM / SIGKILL) to communicate with a solver in accordance with the [optil.io](https://www.optil.io/optilion/help) rules, we are unaware of a mechanism to port the runner to MS Windows.
If you are developing under Windows consider using the [Windows Subsystem For Linux](https://learn.microsoft.com/en-us/windows/wsl/install).
As an added benefit, your solver will be automatically compatible with optil.io.
In case you are aware of a better solution, please let us know.
Any help is welcomed -- please file an issue or, even better, a pull request. 

### Getting a binary (Linux only)
As soon, as the code reaches a certain maturity, we will offer a binary release. 
In the mean time, you can download a static linux binary from the [GitHub action of this repo](https://github.com/manpen/stride_runner_domset/actions/workflows/rust.yml). 
Simply follow the link, click on the latest successful run and download `stride-runner_x86_64-unknown-linux-musl` from the *Artifacts* section on the bottom of the page.

### Build it from sources
Building from sources should be rather simple.
The only dependency we need is a [recent Rust installation](https://www.rust-lang.org/learn/get-started).
The installation of the toolchain is usually pretty easy and only require a single command.
Afterwards simply run `cargo build --release` from within the source directory.
This will produce a runner binary in the folder `target/release/runner`. 
That's it ... hopefully ;)

## Using the runner
The runner always operates relative to the current working directory.
For instance, if you want to track two different algorithms under separate Solver UUID and profiles, you can simply start the runner from two different folders to keep all data separate.
**Important:** Please avoid working in folders on network shares as this might result in corrupted databases (see FAQ).

### Setting up a new directory
To setup the runner, the following command(s) suffice(s). 
For simplicity, we assume the runner and solver executables are in the current working directory; this is, however, not required:
```bash
./runner update     # retrieve ~150MB database dumps from server (see below)
./runner register   # OPTIONAL: sets a random solver uuid in `config.json` (see below)
```

With these steps, the runner creates the subfolder `.stride`.
Among others, it contains:
 - `metadata.db`: A [SQLite database](https://www.sqlite.org/) of the metadata of all instances currently available on the website.
 - `instances.db`: A [SQLite database](https://www.sqlite.org/) of some of the instance data; we initially download all tiny graphs in one block and fetch+cache larger instances on demand.
 - `config.json`: Here, you can enter default values for many command-line arguments to avoid typing (e.g., the Solver UUID, Path to Solver Binary, Timeouts, etc ..). This is the only file you might want to backup; everything else can be retrieved again from the server.

### Updates
The `metadata.db` is **not** kept in sync with the server and even your own uploads are not directly reflected in your local copy.
Thus it makes sense to run `./runner update` from time to time.
Observe that the server produces database dumps roughly every 10min; thus, it may take a few minutes for new information to become available.

After the initial execution of `./runner update`, every further call will, by default, only update the metadata (< 5MB data transferred).
If you have good reason, you can pass the `-d`/`--update-instance-data` argument to also update the instance data.
This is option is almost never helpful, as we will never change existing instance data and only add new instance.
Those will be automatically fetched by the runner on demand.

### Executing your solver
The runner implements the same interface prescribed by [PACE](https://pacechallenge.org/2025/ds/) and [optil.io](https://www.optil.io/optilion/help):
 - You have to provide a solver executable (`-b`/`--solver-bin`)
 - It has to read the solution in the [DIMACS format](https://pacechallenge.org/2025/ds/) from STDIN (the first node id is 1)
 - It has to provide the solution via STDOUT (observe that the first non-comment line needs to contain the cardinality of the solution!)
 - You can set a timeout in seconds (`-T`, `--timeout`).
   After this time the runner sends a `SIGTERM` to the solver, which may trigger some output routine. 
   After a grace period (`-G`, `--grace`) the solver is killed and its output disregarded.

The runner will start the solver for each instance of a predefined set.
By default, it executes `k` solvers in parallel where `k` is the number of hardware threads of your CPU.
You can use the `-j` argument to overwrite this setting (e.g., if RAM size is a concern).

Currently, the runner has two ways to select the instances to operate on:
 - Use the `-i`/`--instances` argument to point to a text file which has one Instance ID (IID) per line.
   Such a file can easily be generated using the [STRIDE web interface](https://domset.algorithm.engineering):
   Select a couple of constraints and then use the 'Download' button above the table to retrieve the list.
   You may edit it by hand, if necessary.   
   **Hint**: If you assign a Solver UUID, you can also add constraints based on your solver performance.
   As an example, you can filter particularly bad/slow solver runs and download their IIDs to focus on the pain points ....

 - Use the `-w`/`--where "X"` argument to issue an SQL query against your local database clone.
   This argument will result in the query `SELECT iid FROM Instance WHERE X` (see FAQ for info on the schema).
   If used in combination with `-i`, the solver will consider the intersection of both sources.
   Finally, you can use the `-e` argument to dump the instance into a file.

Examples:
```bash
# execute solver `./solver` on all instances stated in the file `demo.list`
# with a timeout of 30s and a grace period of 5s
./runner run -i demo.list --timeout 30 --grace 5 --solver-bin ./solver

# execute solver `./solver --foo --bar` with same parameters as before
./runner run -i demo.list --timeout 30 --grace 5 --solver-bin ./solver -- --foo --bar

# execute solver `./solver` on all instances with 123 nodes
# with a timeout of 10s and a grace period of 3s
./runner run --where "nodes = 123" -T 10 -G 3 --solver-bin ./solver 

# export a list of all graphs with 42 edges:
./runner run --where "edges = 42" -e edges42.list

# export a list of all planar graphs with either 1337 nodes or 31415 edges:
./runner run --where "bipartite = True AND (nodes = 1337 or edges = 31415)" -e random.list

# export a list of all instances present in `demo.list` with a known treewidth < 4:
./runner run -i demo.list --where "treewidth < 4" -e small_treewidth.list

# execute solver `./solver` on all known instances and highlight 
# suboptimal solutions (see section `Troubleshooting`)
./runner run --where "1=1" --suboptimal-is-error --solver-bin ./solver 

# Show a help for the command itself (first) and the `run` subcommand (second)
./runner --help
./runner run --help
```

### Environment Variables
Unless the `-E`/`--no-env` flag is set, the runner will provide some additional information to the solver by setting environment variables.
This may help you during the development of your solver, but keep in mind that these information are **not** available for PACE.
The following variables will be set:

| Name                | Optional       | Values       |
| ------------------- | -------------- | ------------ |
| `STRIDE_EDGES`      | always present | unsigned int |
| `STRIDE_IID`        | always present | unsigned int |
| `STRIDE_NODES`      | always present | unsigned int |
| `STRIDE_BEST_SCORE` | if available   | unsigned int |
| `STRIDE_BIPARTITE`  | if available   | false, true  |
| `STRIDE_DIAMETER`   | if available   | unsigned int |
| `STRIDE_TREEWIDTH`  | if available   | unsigned int |
| `STRIDE_PLANAR`     | if available   | false, true  |

### Troubleshooting
If you assigned a Solver UUID, you can investigate your solvers performance on the STRIDE website (link is shown by the runner).
By clicking on a run, you are shown the performance on each instance and can sort/filter by criteria, such as error modes or solution quality.

To protect your data (also see below), the runner does not upload logging/debugging information of your solver.
This is kept only locally on your machine.
Once you start a run, the runner creates the directory `stride-logs/{DATE}_{TIME}_{RUN-UUID}`.
For for each instance `i` it places three files into this directory:
 - `iid{i}.stdin.gr`: contains the input fed to your solver
 - `iid{i}.stdout` / `idd{i}.stderr`: the responses of your solver

By default these files will be deleted for all runs which gave a feasible Dominating Set.
Results are only retained for failed/timeout/infeasible runs. 
This behavior can be changed:
 - By passing the `-o`/`--suboptimal-is-error` argument, the runner will highlight suboptimal solutions more prominently during the run and retain the stdin-stdout-stderr triple for all non-optimal solutions.
 - By passing the `-k`/`--keep-logs-on-success` argument, the runner will keep all logs.

In case the runner itself misbehaves, it might help to enable logging by passing the `-l` / `--logging` with values (`info`, `debug`, `trace`) **in front** of the command:

```bash
# enable trace logging (potentially huge file!) and keep all stdin-stdout-stderr triples
./runner --logging trace run -i demo.list -k
```

### Run Summary
The runner will also create the file `summary.csv` within its logging directory `stride-logs/{DATE}_{TIME}_{RUN-UUID}`.
The first line of the CSV file contains the column headers, each following line contains the summary of a job. 

For instance, the following summary contains a single job (instance iid 110) which was solved in roughly 2ms yielding an suboptimal solution of cardinality 8 while the current best known solution has cardinality 7:

```csv
iid,time_sec,state,score,best_score_known
110,0.002624989,suboptimal,8,7
```

The state column may take the following values:
 - `best`: a feasible solution where no better solution is known
 - `suboptimal`: a feasible solution where a smaller solution is known
 - `infeasible`: a syntactically correct solution that is not a valid dominating set
 - `incomplete`: no solution was provided / a partial solution was provided which had fewer nodes that indicated in the first line.
   This could be due to an too slow output routine.
 - `error`: the runner terminated with a non zero exit code or, in very rare cases, the runner encountered an internal error
 - `timeout`: the solver did not terminate within the grace period

## Data protection
**We are not interested in your personal data** and designed the whole system in good faith to collect as little data as possible while still achieving the goals:
- Your solver never leaves your machine
- The runner only uploads normalized solutions (sorted and without comments) and metadata, such as runtime, error codes and solution scores
- There is **no registration** process where we ask for your name, mail, or affiliation.
  If you want to track your solutions over time, you can **opt-in** by selecting a random *Solver UUID*.
  This UUID is the only identification:
  - it cannot be recovered if you lose it
  - anybody who knows it can access your uploads
- You can annotate each run with a name and description; this information will not be included in the final data dump
- We do not store your IP address in our database (though we may enable short-term access logs for debugging purposes)
- Until the end of the PACE challenge, we, the operators, will only use the publicly information ourselves

## Acknowledgments
We only offer the best organically grown and locally sourced problem instances.
We would like to explicitly thank largest suppliers 
 - [PACE](https://pacechallenge.org/) from which we took complete instances.
 - [Network Repository](https://pacechallenge.org/) from which we extracted a large number of connected components.
   If the original graph was directed, we ignored the directionality.
   Additionally we deleted parallel edges and self loops.
 - [NetworX](https://networkx.org/) with which we generated synthetic instances such as meshes 
  
The exact source is noted on the ~~label~~ description of the instance; 
please cite them, if you are using their data and respect their license.

To compute graph features we used the following software libraries:
 - [NetworKIT](https://networkit.github.io/) for processing potentially large graphs of Network Repository and extract CCs.
 - [NetworX](https://networkx.org/) for "measuring" smaller graphs (e.g., planarity tests etc)
 - [The exact solver of H. Tamaki](https://github.com/twalgor/tw/tree/master) to compute optimal tree-decomposition for the instances.
   We only report treewidth for instances, where this solver succeeded within ~3h.

## FAQ

### Getting in touch
- **Q:** How can I contact you?

  Please feel free to contact us using:
    - An Issue / Pull Request on GitHub in the [Runner Repository](https://github.com/manpen/stride_runner_domset/)
    - An Issue / Pull Request on GitHub in the [Server Repository](https://github.com/manpen/stride_server_domset/)
    - An E-Mail to Manuel at `stride@algorithm.engineering`
    - ... ?

- **Q:** Can I add problem instances?

  YES, we'd love to have your problems ;) Just write an E-Mail to Manuel.

### Database

- **Q:** Help! I accidentally typed `; DROP TABLE Instance` during my SQL query.
  
  Well that happens to best of us ... simply delete `.stride/metadata.db` (and if necessary `.stride/instance.db`) and run `./runner update`.

- **Q:** There are SQLite errors due to corrupted database images.

  Recover using the steps above. We encountered that problem once, when multiple runners were executed on a network mount.

- **Q:** What columns can I query against?

  At time of writing, the `Instance` Table has the following columns:
```sql
CREATE TABLE IF NOT EXISTS "Instance" (
        "iid" INTEGER PRIMARY KEY AUTOINCREMENT,
        "data_did" INTEGER NOT NULL  ,
        "nodes" INTEGER NOT NULL  ,
        "edges" INTEGER NOT NULL  ,
        "name" VARCHAR(255) NULL  ,
        "description" TEXT NULL  ,
        "submitted_by" VARCHAR(255) NULL  ,
        "created_at" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ,
        "min_deg" INTEGER NULL  ,
        "max_deg" INTEGER NULL  ,
        "num_ccs" INTEGER NULL  ,
        "nodes_largest_cc" INTEGER NULL  ,
        "planar" TINYINT NULL  ,
        "bipartite" TINYINT NULL  ,
        "diameter" INTEGER NULL  ,
        "treewidth" INTEGER NULL  ,
        "best_score" INTEGER NULL
);
```

  It is an ordinary SQLite stored in the file `.stride/metadata.db`.
  Feel free to interact with the DB in different ways, e.g., using the `sqlite3 .stride/metadata.db`.

