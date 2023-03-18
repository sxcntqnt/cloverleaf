use simple_grad::*;
use hashbrown::HashMap;
use rand::prelude::*;

use crate::FeatureStore;
use crate::EmbeddingStore;
use crate::graph::{Graph as CGraph,NodeID};

pub trait Model: Send + Sync {

    fn construct_node_embedding<R: Rng>(
        &self,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode);

    fn reconstruct_node_embedding<G: CGraph, R: Rng>(
        &self,
        graph: &G,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode);

    fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
        &self,
        nodes: I,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode); 

    fn parameters(&self) -> Vec<ANode>;
}

pub struct AveragedFeatureModel {
    max_features: Option<usize>,
    max_neighbor_nodes: Option<usize>
}

impl AveragedFeatureModel {
    pub fn new(
        max_features: Option<usize>,
        max_neighbor_nodes: Option<usize>
    ) -> Self {
        AveragedFeatureModel { max_features, max_neighbor_nodes }
    }
}

impl Model for AveragedFeatureModel {
    fn construct_node_embedding<R: Rng>(
        &self,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) {
        construct_node_embedding(
            node,
            feature_store,
            feature_embeddings,
            self.max_features,
            rng)
    }

    fn reconstruct_node_embedding<G: CGraph, R: Rng>(
        &self,
        graph: &G,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode){
        reconstruct_node_embedding(
            graph,
            node,
            feature_store,
            feature_embeddings,
            self.max_neighbor_nodes,
            self.max_features,
            rng)
    }

    fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
        &self,
        nodes: I,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) { 
        let mut feature_map = HashMap::new();
        for node in nodes {
            collect_embeddings_from_node(node, feature_store, 
                                         feature_embeddings, 
                                         &mut feature_map,
                                         self.max_features,
                                         rng);
        }
        let mean = mean_embeddings(feature_map.values());
        (feature_map, mean)
    }

    fn parameters(&self) -> Vec<ANode> {
        Vec::with_capacity(0)
    }
 
}

pub struct AttentionFeatureModel {
    max_features: Option<usize>,
    max_neighbor_nodes: Option<usize>
}

impl AttentionFeatureModel {
    pub fn new(
        max_features: Option<usize>,
        max_neighbor_nodes: Option<usize>
    ) -> Self {
        AttentionFeatureModel { max_features, max_neighbor_nodes }
    }
}

impl Model for AttentionFeatureModel {
    fn construct_node_embedding<R: Rng>(
        &self,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) {
        attention_construct_node_embedding(
            node,
            feature_store,
            feature_embeddings,
            self.max_features,
            rng)
    }

    fn reconstruct_node_embedding<G: CGraph, R: Rng>(
        &self,
        graph: &G,
        node: NodeID,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode){
        reconstruct_node_embedding(
            graph,
            node,
            feature_store,
            feature_embeddings,
            self.max_neighbor_nodes,
            self.max_features,
            rng)
    }

    fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
        &self,
        nodes: I,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) { 
        let mut feature_map = HashMap::new();
        for node in nodes {
            collect_embeddings_from_node(node, feature_store, 
                                         feature_embeddings, 
                                         &mut feature_map,
                                         self.max_features,
                                         rng);
        }
        let mean = mean_embeddings(feature_map.values());
        (feature_map, mean)
    }

    fn parameters(&self) -> Vec<ANode> {
        Vec::with_capacity(0)
    }
 
}

pub type NodeCounts = HashMap<usize, (ANode, usize)>;

pub fn collect_embeddings_from_node<R: Rng>(
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    feat_map: &mut NodeCounts,
    max_features: Option<usize>,
    rng: &mut R
) {
    let feats = feature_store.get_features(node);
    let max_features = max_features.unwrap_or(feats.len());
    for feat in feats.choose_multiple(rng, max_features) {
        if let Some((_emb, count)) = feat_map.get_mut(feat) {
            *count += 1;
        } else {
            let emb = feature_embeddings.get_embedding(*feat);
            let v = Variable::pooled(emb);
            feat_map.insert(*feat, (v, 1));
        }
    }
}

// H(n)
// Average the features associated with a node
// to create the node embedding
pub fn construct_node_embedding<R: Rng>(
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_features: Option<usize>,
    rng: &mut R
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    collect_embeddings_from_node(node, feature_store, 
                                 feature_embeddings, 
                                 &mut feature_map,
                                 max_features,
                                 rng);

    let mean = mean_embeddings(feature_map.values());
    (feature_map, mean)
}

// Attention H(n)
// Use scaled attention between features associated with a node
// to create the node embedding
pub fn attention_construct_node_embedding<R: Rng>(
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_features: Option<usize>,
    rng: &mut R
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    collect_embeddings_from_node(node, feature_store, 
                                 feature_embeddings, 
                                 &mut feature_map,
                                 max_features,
                                 rng);

    let mean = attention_mean(feature_map.values());
    (feature_map, mean)
}

pub fn attention_mean<'a>(
    it: impl Iterator<Item=&'a (ANode, usize)>
) -> ANode {
    let items: Vec<_> = it.collect();
    if items.len() == 1 {
        return items[0].0.clone()
    }
    
    // Get the attention for each feature
    let mut scaled = vec![Vec::with_capacity(items.len()); items.len()];
    for i in 0..items.len() {
        for j in (i+1)..items.len() {
            let (iv, ic) = items[i];
            let (jv, jc) = items[j];
            let dot = (&iv).dot(&jv);
            let sdot = dot * (ic * jc) as f32;
            scaled[i].push(sdot.clone());
            scaled[j].push(sdot);
        }
    }

    // Compute softmax
    let d_k = Constant::scalar((scaled[0][0].value().len() as f32).sqrt());
    let exps: Vec<_> = scaled.into_iter()
        .map(|dots| (dots.sum_all() / &d_k).exp())
        .collect();

    let denom = exps.clone().sum_all();
    let softmax = exps.into_iter().map(|v| v / &denom);
    items.into_iter().zip(softmax)
        .map(|((feat, _c), attention)| feat * attention)
        .collect::<Vec<_>>().sum_all()
}

// ~H(n)
// The Expensive function.  We grab a nodes neighbors
// and use the average of their features to construct
// an estimate of H(n)
pub fn reconstruct_node_embedding<G: CGraph, R: Rng>(
    graph: &G,
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_nodes: Option<usize>,
    max_features: Option<usize>,
    rng: &mut R
) -> (NodeCounts, ANode) {
    let edges = &graph.get_edges(node).0;
    
    if edges.len() <= max_nodes.unwrap_or(edges.len()) {
        construct_from_multiple_nodes(edges.iter().cloned(),
            feature_store,
            feature_embeddings,
            max_features,
            rng)
    } else {
        let it = edges.choose_multiple(rng, max_nodes.unwrap()).cloned();
        construct_from_multiple_nodes(it,
            feature_store,
            feature_embeddings,
            max_features,
            rng)
    }
}

pub fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
    nodes: I,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_features: Option<usize>,
    rng: &mut R
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    for node in nodes {
        collect_embeddings_from_node(node, feature_store, 
                                     feature_embeddings, 
                                     &mut feature_map,
                                     max_features,
                                     rng);
    }
    let mean = mean_embeddings(feature_map.values());
    (feature_map, mean)
}

pub fn mean_embeddings<'a,I: Iterator<Item=&'a (ANode, usize)>>(items: I) -> ANode {
    let mut vs = Vec::new();
    let mut n = 0;
    items.for_each(|(emb, count)| {
        if *count > 1 {
            vs.push(emb * *count as f32);
        } else {
            vs.push(emb.clone());
        }
        n += *count;
    });
    vs.sum_all() / n as f32
}


