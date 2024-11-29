pub type Node = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge(pub Node, pub Node);

impl Edge {
    pub fn new(u: Node, v: Node) -> Self {
        Self(u, v)
    }

    pub fn normalized(&self) -> Self {
        if self.0 < self.1 {
            *self
        } else {
            Self(self.1, self.0)
        }
    }

    pub fn max_node(&self) -> Node {
        self.0.max(self.1)
    }

    pub fn min_node(&self) -> Node {
        self.0.min(self.1)
    }
}

pub type NumNodes = Node;
pub type NumEdges = u64;
