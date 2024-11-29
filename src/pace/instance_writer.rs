use super::graph::*;
use std::io::Write;

pub fn pace_writer<W: Write>(
    mut writer: W,
    problem_id: &str,
    edges: impl Iterator<Item = Edge>,
) -> Result<(NumNodes, NumEdges), std::io::Error> {
    let mut edges: Vec<Edge> = edges.collect();

    if edges.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "No edges to write",
        ));
    }

    edges.sort_unstable();
    edges.dedup();

    let max_node = edges
        .iter()
        .map(|Edge(u, v)| std::cmp::max(*u, *v))
        .max()
        .unwrap_or(0);

    let min_node = edges
        .iter()
        .map(|Edge(u, v)| std::cmp::min(*u, *v))
        .min()
        .unwrap_or(0);

    let num_nodes = max_node - min_node + 1;
    let num_edges = edges.len();

    writeln!(writer, "p {problem_id} {num_nodes} {num_edges}")?;

    for Edge(u, v) in edges {
        writeln!(writer, "{} {}", u - min_node + 1, v - min_node + 1)?;
    }

    Ok((num_nodes as NumNodes, num_edges as NumEdges))
}

#[cfg(test)]
mod test {
    use super::super::instance_reader::*;

    use super::*;

    #[test]
    fn transcribe() {
        const PROBLEM_ID: &str = "test";
        let mut edges = vec![
            Edge(0, 1),
            Edge(1, 2),
            Edge(2, 3),
            Edge(3, 4),
            Edge(4, 1),
            Edge(4, 0),
            Edge(0, 1),
        ];
        let mut buffer: Vec<u8> = Vec::new();

        let (n, m) = pace_writer(&mut buffer, PROBLEM_ID, edges.iter().copied()).unwrap();

        assert_eq!(n, 5);
        assert_eq!(m, 6);

        // read back written instance
        {
            let reader = PaceReader::try_new(&buffer[..]).unwrap();

            assert_eq!(reader.problem_id(), PROBLEM_ID);
            assert_eq!(reader.number_of_nodes(), n);
            assert_eq!(reader.number_of_edges(), m);

            edges.sort();
            edges.dedup();

            let read_edges: Vec<_> = reader.map(|x| x.unwrap()).collect();

            assert_eq!(edges, read_edges);
        }
    }
}
