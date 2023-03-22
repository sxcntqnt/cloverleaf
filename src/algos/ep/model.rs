use simple_grad::*;
use hashbrown::HashMap;
use rand::prelude::*;
use float_ord::FloatOrd;

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
            0,
            rng)
    }

    fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
        &self,
        nodes: I,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) { 
        construct_from_multiple_nodes(
            nodes, feature_store, 
            feature_embeddings, 
            self.max_features,
            0, rng)
    }

    fn parameters(&self) -> Vec<ANode> {
        Vec::with_capacity(0)
    }
 
}

pub struct AttentionFeatureModel {
    dims: usize,
    max_features: Option<usize>,
    max_neighbor_nodes: Option<usize>
}

impl AttentionFeatureModel {
    pub fn new(
        dims: usize,
        max_features: Option<usize>,
        max_neighbor_nodes: Option<usize>
    ) -> Self {
        AttentionFeatureModel { dims, max_features, max_neighbor_nodes }
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
            self.dims,
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
            self.dims,
            rng)
    }

    fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
        &self,
        nodes: I,
        feature_store: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (NodeCounts, ANode) { 
        construct_from_multiple_nodes(
            nodes, feature_store, 
            feature_embeddings, 
            self.max_features,
            self.dims, rng)
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
fn construct_node_embedding<R: Rng>(
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
    attention_dims: usize,
    rng: &mut R
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    collect_embeddings_from_node(node, feature_store, 
                                 feature_embeddings, 
                                 &mut feature_map,
                                 max_features,
                                 rng);

    let mean = attention_mean(feature_map.values(), attention_dims);
    (feature_map, mean)
}

pub fn attention_mean<'a>(
    it: impl Iterator<Item=&'a (ANode, usize)>,
    attention_dims: usize
) -> ANode {

    let items: Vec<_> = it.map(|(node, count)| {
        let query = get_query_vec(&node, attention_dims);
        let key = get_key_vec(&node, attention_dims);
        let value = get_value_vec(&node, attention_dims);
        (value, count, query, key)
    }).collect();

    if items.len() == 1 {
        return get_value_vec(&items[0].0, attention_dims)
    }
    
    // Get the attention for each feature
    let mut scaled = vec![Vec::with_capacity(items.len()); items.len()];
    for i in 0..items.len() {
        for j in (i+1)..items.len() {
            let (_, ic, qvi, kvi) = &items[i];
            let (_, jc, qvj, kvj) = &items[j];
            let mut dot_i_j = (&qvi).dot(&kvj);
            let mut dot_j_i = (&qvj).dot(&kvi);
            let num = **ic * **jc;
            if num >= 1 {
                let scale = Constant::scalar(num as f32);
                dot_i_j = dot_i_j * &scale;
                dot_j_i = dot_j_i * scale;
            }
            scaled[i].push(dot_i_j);
            scaled[j].push(dot_j_i);
        }
    }

    // Compute softmax
    let d_k = Constant::scalar((scaled[0][0].value().len() as f32).sqrt());

    let mut numers: Vec<_> = scaled.into_iter()
        .map(|dots| dots.sum_all() / &d_k)
        .collect();

    let max_value = numers.iter().map(|v| v.value()[0])
        .max_by_key(|v| FloatOrd(*v))
        .expect("Shouldn't be non-zero!");

    let mv = Constant::scalar(max_value);
    numers.iter_mut().for_each(|v| {
        *v = ((&*v) - &mv).exp()
    });

    let denom = numers.clone().sum_all();
    let softmax = numers.into_iter().map(|v| v / &denom);
    items.into_iter().zip(softmax)
        .map(|((value, _c, _ , _), attention)| value * attention)
        .collect::<Vec<_>>().sum_all()
 }

// ~H(n)
// The Expensive function.  We grab a nodes neighbors
// and use the average of their features to construct
// an estimate of H(n)
fn reconstruct_node_embedding<G: CGraph, R: Rng>(
    graph: &G,
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_nodes: Option<usize>,
    max_features: Option<usize>,
    attention_dims: usize, 
    rng: &mut R
) -> (NodeCounts, ANode) {
    let edges = &graph.get_edges(node).0;
    
    if edges.len() <= max_nodes.unwrap_or(edges.len()) {
        construct_from_multiple_nodes(edges.iter().cloned(),
            feature_store,
            feature_embeddings,
            max_features,
            attention_dims,
            rng)
    } else {
        let it = edges.choose_multiple(rng, max_nodes.unwrap()).cloned();
        construct_from_multiple_nodes(it,
            feature_store,
            feature_embeddings,
            max_features,
            attention_dims,
            rng)
    }
}

fn construct_from_multiple_nodes<I: Iterator<Item=NodeID>, R: Rng>(
    nodes: I,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_features: Option<usize>,
    attention_dims:usize,
    rng: &mut R,
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    let mut new_nodes = Vec::with_capacity(0);
    for node in nodes {
        if attention_dims > 0 {
            new_nodes.push(node.clone());
        }

        collect_embeddings_from_node(node, feature_store, 
                                     feature_embeddings, 
                                     &mut feature_map,
                                     max_features,
                                     rng);
    }

    let mean = if attention_dims == 0 {
        mean_embeddings(feature_map.values())
    } else {
        attention_multiple(new_nodes, feature_store, &feature_map, attention_dims)
    };
    (feature_map, mean)
}

fn get_value_vec(emb: &ANode, dims: usize) -> ANode {
    let v = emb.value().len();
    emb.slice(2*dims, v - 2*dims)
}

fn get_query_vec(emb: &ANode, dims: usize) -> ANode {
    emb.slice(0, dims)
}

fn get_key_vec(emb: &ANode, dims: usize) -> ANode {
    emb.slice(dims, dims)
}

fn attention_multiple(
    new_nodes: Vec<NodeID>,
    feature_store: &FeatureStore,
    feature_map: &NodeCounts,
    attention_dims: usize
) -> ANode {
    let mut feats_per_node = HashMap::new();
    let mut output = Vec::new();
    for node in new_nodes {
        feats_per_node.clear();
        for feat in feature_store.get_features(node).iter() {
            if let Some((node, _)) = feature_map.get(feat) {
                let e = feats_per_node.entry(feat).or_insert_with(|| (node.clone(), 0usize));
                e.1 += 1;
            }
        }
        output.push((attention_mean(feats_per_node.values(), attention_dims), 1))
    }
    mean_embeddings(output.iter())
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


