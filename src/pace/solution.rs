use sha1::{digest::Output, Digest, Sha1};

use super::graph::*;
use std::{
    collections::HashSet,
    io::{BufRead, Write},
};

pub type Result<T> = std::io::Result<T>;

pub struct Solution {
    pub solution: Vec<Node>,
}

impl Solution {
    pub fn from_0indexed_vec(solution: Vec<Node>) -> Self {
        Self { solution }
    }

    pub fn from_1indexed_vec(
        mut solution: Vec<Node>,
        nodes_upper_bound: Option<NumNodes>,
    ) -> Result<Self> {
        for u in solution.iter_mut() {
            if *u == 0 || nodes_upper_bound.is_some_and(|n| *u > n) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Node id out of range",
                ));
            }

            *u -= 1;
        }

        Ok(Self { solution })
    }

    pub fn read<R: BufRead>(reader: R, nodes_upper_bound: Option<NumNodes>) -> Result<Self> {
        let reader = reader::SolutionReader::try_new(reader)?;
        let solution_size = reader.solution_size();

        let mut solution = Vec::with_capacity(reader.solution_size() as usize);

        // read all nodes in solution
        for node in reader {
            let node = node?;

            if nodes_upper_bound.is_some_and(|x| node >= x) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Node id larger than the number of nodes in the header",
                ));
            }

            solution.push(node);
        }

        if solution.len() != solution_size as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Number of nodes in solution does not match the number of nodes in the header",
            ));
        }

        // sort and deduplicate solution
        solution.sort_unstable();
        solution.dedup();

        if solution.len() != solution_size as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Solution contains duplicates",
            ));
        }

        Ok(Self { solution })
    }

    pub fn solution(&self) -> &[Node] {
        &self.solution
    }

    pub fn write<W: Write>(&self, mut writer: W) -> Result<()> {
        if self.solution.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No solution to write",
            ));
        }

        writeln!(writer, "{}", self.solution.len())?;

        for &u in &self.solution {
            writeln!(writer, "{}", u + 1)?;
        }

        Ok(())
    }

    /// Verifies that the solution is a valid dominating set for the given graph.
    pub fn valid_domset_for_instance(
        &self,
        n: NumNodes,
        edges: impl Iterator<Item = Edge>,
    ) -> Result<bool> {
        // TODO: we are building a complete adj list; we should refactor that out!
        let mut adjlist = (0..n).map(|_| Vec::new()).collect::<Vec<_>>();
        for Edge(u, v) in edges {
            if u.max(v) >= n {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Edge contains node id larger than the number of nodes",
                ));
            }

            adjlist[u as usize].push(v);
            adjlist[v as usize].push(u);
        }

        let mut covered = HashSet::<Node>::with_capacity(n as usize);
        for &u in &self.solution {
            if u >= n {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Node id in solution larger than the number of nodes",
                ));
            }

            covered.insert(u);
            covered.extend(adjlist[u as usize].iter().copied());
        }

        Ok(covered.len() == n as usize)
    }

    pub fn compute_digest(&self) -> Output<Sha1> {
        let mut hasher = Sha1::new();

        for &node in &self.solution {
            hasher.update((node + 1).to_le_bytes());
        }

        hasher.finalize()
    }

    pub fn take_solution(self) -> Vec<Node> {
        self.solution
    }

    pub fn take_1indexed_solution(mut self) -> Vec<Node> {
        for x in &mut self.solution {
            *x += 1;
        }

        self.solution
    }
}

mod reader {
    use super::*;
    use std::io::Lines;

    pub struct SolutionReader<R> {
        lines: Lines<R>,
        solution_size: NumNodes,
    }

    #[allow(dead_code)]
    impl<R: BufRead> SolutionReader<R> {
        pub fn try_new(reader: R) -> Result<Self> {
            let mut reader = Self {
                lines: reader.lines(),
                solution_size: 0,
            };

            reader.solution_size = match reader.next_non_comment_line() {
                Some(Ok(x)) => x,
                Some(Err(e)) => return Err(e),
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "No solution size found",
                    ))
                }
            };

            Ok(reader)
        }

        pub fn solution_size(&self) -> NumNodes {
            self.solution_size
        }

        fn next_non_comment_line(&mut self) -> Option<Result<Node>> {
            loop {
                let line = match self.lines.next()? {
                    Ok(x) => x,
                    Err(e) => return Some(Err(e)),
                };

                let trimmed_line = line.trim();
                if trimmed_line.is_empty() || trimmed_line.starts_with('c') {
                    // empty lines are not mentioned in the spec, but we allow them
                    continue;
                }

                if trimmed_line.chars().any(|c| !c.is_numeric()) {
                    return Some(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Non-numeric character in line",
                    )));
                }

                return Some(
                    trimmed_line
                        .parse::<Node>()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                );
            }
        }
    }

    impl<R: BufRead> Iterator for SolutionReader<R> {
        type Item = Result<Node>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.next_non_comment_line() {
                Some(Ok(x)) if x > 0 => Some(Ok(x - 1)),
                Some(Ok(_)) => Some(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Node id smaller than 1",
                ))),
                Some(Err(e)) => Some(Err(e)),
                None => None,
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn solution_reader_legal() {
            let data = "5\n1\n2\n3\n4\n6\n";
            let reader = SolutionReader::try_new(data.as_bytes()).unwrap();
            assert_eq!(reader.solution_size(), 5);
            let result: Vec<Node> = reader.map(Result::unwrap).collect();
            assert_eq!(result, vec![0, 1, 2, 3, 5]);
        }

        #[test]
        fn solution_reader_with_comment() {
            let data = "c Test\n5\n1\n2\ncBla\n3\n4\n5\n";
            let reader = SolutionReader::try_new(data.as_bytes()).unwrap();
            let result: Vec<Node> = reader.map(Result::unwrap).collect();
            assert_eq!(result, vec![0, 1, 2, 3, 4]);
        }

        #[test]
        fn solution_reader_illegal() {
            let data = "5\n1\n2\na\n1 2\n3\n";
            let mut reader = SolutionReader::try_new(data.as_bytes()).unwrap();

            assert_eq!(reader.next().unwrap().unwrap(), 0);
            assert_eq!(reader.next().unwrap().unwrap(), 1);
            assert!(reader.next().unwrap().is_err());
            assert!(reader.next().unwrap().is_err());
            assert_eq!(reader.next().unwrap().unwrap(), 2);
            assert!(reader.next().is_none());
            assert!(reader.next().is_none());
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn transcribe() {
        for n in 1..100 {
            let mut input = (0..n).rev().collect::<Vec<Node>>();
            let ref_solution = Solution {
                solution: input.clone(),
            };

            let mut buffer = Vec::new();
            ref_solution.write(&mut buffer).unwrap();

            let read_back_solution = Solution::read(buffer.as_slice(), Some(n)).unwrap();

            input.reverse();

            assert_eq!(read_back_solution.solution(), input.as_slice());
        }
    }

    #[test]
    fn test_domset_verifier() {
        let edges = [Edge(0, 1), Edge(2, 3)];

        assert!(!Solution { solution: vec![0] }
            .valid_domset_for_instance(4, edges.iter().copied())
            .unwrap());

        assert!(Solution {
            solution: vec![0, 2]
        }
        .valid_domset_for_instance(4, edges.iter().copied())
        .unwrap());
    }

    #[test]
    fn digest_matches_python() {
        let solution = Solution::from_1indexed_vec((1..10).collect(), None).unwrap();
        assert_eq!(
            format!("{:x}", solution.compute_digest()),
            "89d9014041ecdcfe552d725a76a07395d272bded"
        );
    }
}
