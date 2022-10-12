use rayon::prelude::*;
use hashbrown::HashMap;
use rand::prelude::*;
use rand_xorshift::XorShiftRng;
use rand_distr::{Distribution,Uniform};
use simple_grad::*;

use crate::graph::{Graph as CGraph,NodeID};
use crate::embeddings::{EmbeddingStore,Distance};
use crate::vocab::Vocab;

#[derive(Debug)]
pub struct FeatureStore {
    features: Vec<Vec<usize>>,
    feature_vocab: Vocab,
    empty_nodes: usize
}

impl FeatureStore {
    pub fn new(size: usize) -> Self {
        FeatureStore {
            features: vec![Vec::with_capacity(0); size],
            feature_vocab: Vocab::new(),
            empty_nodes: 0
        }
    }

    pub fn set_features(&mut self, node: NodeID, node_features: Vec<String>) {
        self.features[node] = node_features.into_iter()
            .map(|f| self.feature_vocab.get_or_insert("feat".to_string(), f))
            .collect()
    }

    pub fn get_features(&self, node: NodeID) -> &[usize] {
        &self.features[node]
    }

    pub fn len(&self) -> usize {
        self.feature_vocab.len() + self.empty_nodes
    }

    pub fn fill_missing_nodes(&mut self) {
        let mut idxs = self.feature_vocab.len();
        self.features.iter_mut().for_each(|f| {
            if f.len() == 0 {
                *f = vec![idxs];
                idxs += 1;
                self.empty_nodes += 1;
            }
        });
    }

    pub fn get_vocab(self) -> Vocab {
        self.feature_vocab
    }
}

pub struct EmbeddingPropagation {
    pub alpha: f32,
    pub gamma: f32,
    pub batch_size: usize,
    pub dims: usize,
    pub passes: usize,
    pub seed: u64
}

impl EmbeddingPropagation {

    pub fn learn<G: CGraph + Send + Sync>(
        &self, 
        graph: &G, 
        features: &FeatureStore
    ) -> (EmbeddingStore, EmbeddingStore) {
        let mut agraph = Graph::new();
        let feat_embeds = self.learn_feature_embeddings(graph, &mut agraph, features);
        let mut es = EmbeddingStore::new(graph.len(), self.dims, Distance::Cosine);
        for node in 0..graph.len() {
            let node_embedding = construct_node_embedding(node, features, &feat_embeds).1;
            let embedding = es.get_embedding_mut(node);
            embedding.clone_from_slice(node_embedding.value());
        }
        (es, feat_embeds)
    }

    fn learn_feature_embeddings<G: CGraph + Send + Sync>(
        &self,
        graph: &G,
        agraph: &mut Graph,
        features: &FeatureStore,
    ) -> EmbeddingStore {

        let mut feature_embeddings = EmbeddingStore::new(features.len(), self.dims, Distance::Cosine);
        let mut rng = XorShiftRng::seed_from_u64(self.seed);
        randomize_embedding_store(&mut feature_embeddings, &mut rng);

        let mut node_idxs: Vec<_> = (0..graph.len()).into_iter().collect();
        let dist = Uniform::new(0, node_idxs.len());
        // Enable/disable shared memory pool
        use_shared_pool(true);
        //use_shared_pool(self.batch_size > 1);

        let mut grads = Vec::with_capacity(self.batch_size);
        let mut all_grads = HashMap::new();
        for pass in 0..self.passes {
            // Shuffle for SGD
            node_idxs.shuffle(&mut rng);
            let mut error = 0f32;
            let mut cnt = 0usize;
            for (i, nodes) in node_idxs.chunks(self.batch_size).enumerate() {
                
                // Compute grads for batch
                nodes.par_iter().map(|node_id| {
                    let mut rng = XorShiftRng::seed_from_u64(self.seed + (i + node_id) as u64);
                    let (loss, grads) = self.run_pass(graph, *node_id, &features, &feature_embeddings, &mut rng) ;
                    (loss, grads)
                }).collect_into_vec(&mut grads);

                // Back propagate and SGD
                all_grads.clear();
                for (err, grad_set) in grads.drain(..nodes.len()) {
                    for (feat, grad) in grad_set.into_iter() {
                        let e = all_grads.entry(feat).or_insert_with(|| vec![0.; grad.len()]);
                        e.iter_mut().zip(grad.iter()).for_each(|(ei, gi)| *ei += *gi);
                    }
                    error += err;
                    cnt += 1;
                }
                sgd(&mut feature_embeddings, &mut all_grads, self.alpha);

            }
            eprintln!("Pass: {}, Error: {:.3}", pass, error / node_idxs.len() as f32);
        }
        feature_embeddings
    }

    fn run_pass<G: CGraph + Send + Sync, R: Rng>(
        &self, 
        graph: &G,
        node: NodeID,
        features: &FeatureStore,
        feature_embeddings: &EmbeddingStore,
        rng: &mut R
    ) -> (f32, HashMap<usize, Vec<f32>>) {

        let dist = Uniform::new(0, graph.len());

        // Get negative v
        let neg_node = loop {
            let neg_node = dist.sample(rng);
            if neg_node != node { break neg_node }
        };

        // h(v)
        let (hv_vars, hv) = construct_node_embedding(node, features, &feature_embeddings);
        
        // ~h(v)
        let (thv_vars, thv) = reconstruct_node_embedding(graph, node, features, &feature_embeddings, Some(10));
        
        // h(u)
        let (hu_vars, hu) = construct_node_embedding(neg_node, features, &feature_embeddings);

        // Compute error
        let loss = margin_loss(thv, hv, hu, self.gamma);

        let mut agraph = Graph::new();
        agraph.backward(&loss);

        let mut grads = HashMap::new();
        extract_grads(&agraph, &mut grads, hv_vars.into_iter());
        extract_grads(&agraph, &mut grads, thv_vars.into_iter());
        extract_grads(&agraph, &mut grads, hu_vars.into_iter());

        (loss.value()[0], grads)

    }

}

fn extract_grads(
    graph: &Graph, 
    grads: &mut HashMap<usize, Vec<f32>>, 
    vars: impl Iterator<Item=(usize, (ANode, usize))>
) {
    for (feat_id, (var, _)) in vars {
        if grads.contains_key(&feat_id) { continue }

        let grad = graph.get_grad(&var)
            .expect("Should have a gradient!");

        if grad.iter().all(|gi| !gi.is_nan()) {
            // Can get some nans in weird cases, such as the distance between
            // a node and it's reconstruction when it shares all features.
            // SGD
            grads.insert(feat_id, grad.to_vec());
        }
    }
}

type NodeCounts = HashMap<usize, (ANode, usize)>;
fn sgd(
    feature_embeddings: &mut EmbeddingStore,
    grads: &mut HashMap<usize, Vec<f32>>,
    alpha: f32
) {
    for (feat_id, grad) in grads.drain() {

        let emb = feature_embeddings.get_embedding_mut(feat_id);

        if grad.iter().all(|gi| !gi.is_nan()) {
            // Can get some nans in weird cases, such as the distance between
            // a node and it's reconstruction when it shares all features.
            // SGD
            emb.iter_mut().zip(grad.iter()).for_each(|(ei, gi)| {
                *ei -= alpha * *gi;
            });
        }
    }

}

fn collect_embeddings_from_node(
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    feat_map: &mut NodeCounts  
) {
   for feat in feature_store.get_features(node).iter() {
        if let Some((_emb, count)) = feat_map.get_mut(feat) {
            *count += 1;
        } else {
            let emb = feature_embeddings.get_embedding(*feat);
            let v = Variable::new(emb.to_vec());
            feat_map.insert(*feat, (v, 1));
        }
    }
}

// H(n)
fn construct_node_embedding(
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
) -> (NodeCounts, ANode) {
    let mut feature_map = HashMap::new();
    collect_embeddings_from_node(node, feature_store, 
                                 feature_embeddings, &mut feature_map);
    let mean = mean_embeddings(feature_map.values());
    (feature_map, mean)
}

// ~H(n)
fn reconstruct_node_embedding<G: CGraph>(
    graph: &G,
    node: NodeID,
    feature_store: &FeatureStore,
    feature_embeddings: &EmbeddingStore,
    max_nodes: Option<usize>
) -> (NodeCounts, ANode) {
    let edges = &graph.get_edges(node).0;
    let mut feature_map = HashMap::new();
    for out_node in edges.iter().take(max_nodes.unwrap_or(edges.len())) {
        collect_embeddings_from_node(*out_node, feature_store, 
                                     feature_embeddings, &mut feature_map);
    }
    let mean = mean_embeddings(feature_map.values());
    (feature_map, mean)
}

fn mean_embeddings<'a,I: Iterator<Item=&'a (ANode, usize)>>(items: I) -> ANode {
    let mut vs = Vec::new();
    let mut n = 0;
    items.for_each(|(emb, count)| {
        vs.push(emb * *count as f32);
        n += *count;
    });
    vs.sum_all() / n as f32
}

fn euclidean_distance(e1: ANode, e2: ANode) -> ANode {
    (e1 - e2).pow(2f32).sum().pow(0.5)
}

fn margin_loss(thv: ANode, hv: ANode, hu: ANode, gamma: f32) -> ANode {
    let d1 = euclidean_distance(thv.clone(), hv);
    let d2 = euclidean_distance(thv, hu);
    (gamma + d1 - d2).maximum(0f32)
}

fn randomize_embedding_store(es: &mut EmbeddingStore, rng: &mut impl Rng) {
    for idx in 0..es.len() {
        let e = es.get_embedding_mut(idx);
        e.iter_mut().for_each(|ei| *ei = 2f32 * rng.gen::<f32>() - 1f32);
    }
}

#[cfg(test)]
mod ep_tests {
    use super::*;
    use crate::graph::{CumCSR,CSR};

    fn build_star_edges() -> Vec<(usize, usize, f32)> {
        let mut edges = Vec::new();
        let max = 100;
        for ni in 0..max {
            for no in (ni+1)..max {
                edges.push((ni, no, 1f32));
                edges.push((no, ni, 1f32));
            }
        }
        edges
    }

    #[test]
    fn test_euclidean_dist() {
        let x = Variable::new(vec![1f32, 3f32]);
        let y = Variable::new(vec![3f32, 5f32]);
        let dist = euclidean_distance(x, y);
        assert_eq!(dist.value(), &[(8f32).powf(0.5)]);
    }

    #[test]
    fn test_simple_learn_dist() {
        let edges = build_star_edges();
        let csr = CSR::construct_from_edges(edges);
        let ccsr = CumCSR::convert(csr);
        
        let mut feature_store = FeatureStore::new(ccsr.len());
        feature_store.fill_missing_nodes();

        let mut rng = XorShiftRng::seed_from_u64(202220222);
        let mut agraph = Graph::new();

        let ep = EmbeddingPropagation {
            alpha: 1e-2,
            gamma: 1f32,
            batch_size: 32,
            dims: 5,
            passes: 50,
            seed: 202220222
        };

        let embeddings = ep.learn_feature_embeddings(&ccsr, &mut agraph, &feature_store);
        for idx in 0..embeddings.len() {
            let e = embeddings.get_embedding(idx);
            println!("{:?} -> {:?}", idx, e);
        }
    }

}