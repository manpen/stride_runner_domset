use std::io::{BufRead, ErrorKind, Lines};

use super::graph::*;

pub type Result<T> = std::io::Result<T>;

pub struct PaceReader<R> {
    lines: Lines<R>,
    problem_id: String,
    number_of_nodes: NumNodes,
    number_of_edges: NumEdges,
}

#[allow(dead_code)]
impl<R: BufRead> PaceReader<R> {
    pub fn try_new(reader: R) -> Result<Self> {
        let mut pace_reader = Self {
            lines: reader.lines(),
            problem_id: String::new(),
            number_of_nodes: 0,
            number_of_edges: 0,
        };

        (pace_reader.number_of_nodes, pace_reader.number_of_edges) = pace_reader.parse_header()?;
        Ok(pace_reader)
    }

    pub fn number_of_edges(&self) -> NumEdges {
        self.number_of_edges
    }

    pub fn number_of_nodes(&self) -> NumNodes {
        self.number_of_nodes
    }

    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }
}

impl<R: BufRead> Iterator for PaceReader<R> {
    type Item = Result<Edge>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.parse_edge_line() {
            Ok(Some(Edge(u, v))) => Some(Ok(Edge(u - 1, v - 1))),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

macro_rules! raise_error_unless {
    ($cond : expr, $kind : expr, $info : expr) => {
        if !($cond) {
            return Err(std::io::Error::new($kind, $info));
        }
    };
}

macro_rules! parse_next_value {
    ($iterator : expr, $name : expr) => {{
        let next = $iterator.next();
        raise_error_unless!(
            next.is_some(),
            ErrorKind::InvalidData,
            format!("Premature end of line when parsing {}.", $name)
        );

        let parsed = next.unwrap().parse();
        raise_error_unless!(
            parsed.is_ok(),
            ErrorKind::InvalidData,
            format!("Invalid value found. Cannot parse {}.", $name)
        );

        parsed.unwrap()
    }};
}

impl<R: BufRead> PaceReader<R> {
    fn next_non_comment_line(&mut self) -> Result<Option<String>> {
        loop {
            let line = self.lines.next();
            match line {
                None => return Ok(None),
                Some(Err(x)) => return Err(x),
                Some(Ok(line)) if line.starts_with('c') => continue,
                Some(Ok(line)) => return Ok(Some(line)),
            }
        }
    }

    fn parse_header(&mut self) -> Result<(NumNodes, NumEdges)> {
        let line = self.next_non_comment_line()?;

        raise_error_unless!(line.is_some(), ErrorKind::InvalidData, "No header found");
        let line = line.unwrap();

        let mut parts = line.split(' ').filter(|t| !t.is_empty());

        raise_error_unless!(
            parts.next().is_some_and(|t| t.starts_with('p')),
            ErrorKind::InvalidData,
            "Invalid header found; line should start with p"
        );

        match parts.next() {
            Some(x) => {
                self.problem_id = x.to_string();
            }
            _ => {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid header; no problem id found",
                ));
            }
        }

        let number_of_nodes = parse_next_value!(parts, "Header>Number of nodes");
        let number_of_edges = parse_next_value!(parts, "Header>Number of edges");

        raise_error_unless!(
            parts.next().is_none(),
            ErrorKind::InvalidData,
            "Invalid header found; expected end of line"
        );

        Ok((number_of_nodes, number_of_edges))
    }

    fn parse_edge_line(&mut self) -> Result<Option<Edge>> {
        let line = self.next_non_comment_line()?;
        if let Some(line) = line {
            let mut parts = line.split(' ').filter(|t| !t.is_empty());

            let from = parse_next_value!(parts, "Source node");
            let dest = parse_next_value!(parts, "Target node");

            debug_assert!((1..=self.number_of_nodes).contains(&from));
            debug_assert!((1..=self.number_of_nodes).contains(&dest));

            Ok(Some(Edge(from, dest)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_success() {
        const DEMO_FILE: &str =
            "c TEST\n p  deMo 10  9 \n1 2\nc TEST\n2 3\n3 4\n4 5\n5 6\n6 7\n7 8\n8 9\n9 10";
        let buf_reader = std::io::BufReader::new(DEMO_FILE.as_bytes());
        let pace_reader = PaceReader::try_new(buf_reader).unwrap();

        assert_eq!(pace_reader.number_of_nodes(), 10);
        assert_eq!(pace_reader.number_of_edges(), 9);
        assert_eq!(pace_reader.problem_id(), "deMo");

        let edges: Vec<_> = pace_reader.map(|x| x.unwrap()).collect();
        assert_eq!(
            edges,
            vec![
                Edge(0, 1),
                Edge(1, 2),
                Edge(2, 3),
                Edge(3, 4),
                Edge(4, 5),
                Edge(5, 6),
                Edge(6, 7),
                Edge(7, 8),
                Edge(8, 9)
            ]
        );
    }
}
