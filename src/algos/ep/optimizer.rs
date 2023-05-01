use rayon::prelude::*;
use crate::embeddings::{EmbeddingStore,Distance};
use std::collections::{HashMap as CHashMap};

pub trait Optimizer {
    fn update(
        &self, 
        feature_embeddings: &EmbeddingStore,
        grads: CHashMap<usize, Vec<f32>>,
        alpha: f32,
        t: f32
    );

}

pub struct MomentumOptimizer {
    gamma: f32,
    mom: EmbeddingStore
}

impl MomentumOptimizer {

    pub fn new(gamma: f32, dims: usize, length: usize) -> Self {
        let mom = EmbeddingStore::new(length, dims, Distance::Cosine);
        MomentumOptimizer {gamma, mom}
    }
}

impl Optimizer for MomentumOptimizer {

    fn update(
        &self, 
        feature_embeddings: &EmbeddingStore,
        grads: CHashMap<usize, Vec<f32>>,
        alpha: f32,
        _t: f32
    ) {
        for (feat_id, grad) in grads.into_iter() {

            let emb = feature_embeddings.get_embedding_mut_hogwild(feat_id);
            let mom = self.mom.get_embedding_mut_hogwild(feat_id);

            if grad.iter().all(|gi| !gi.is_nan()) {
                // Can get some nans in weird cases, such as the distance between
                // a node and it's reconstruction when it shares all features.
                // We just skip over those weird ones.
                emb.iter_mut().zip(grad.iter().zip(mom.iter_mut())).for_each(|(ei, (gi, mi))| {
                    *mi = self.gamma * *mi + *gi;
                    *ei -= alpha * *mi;
                });
            }
        }
    }

}

pub struct AdamOptimizer {
    beta_1: f32,
    beta_2: f32,
    eps: f32,
    mom: EmbeddingStore,
    var: EmbeddingStore
}

impl AdamOptimizer {
    pub fn new(beta_1: f32, beta_2: f32, dims: usize, length: usize) -> Self {
        let mom = EmbeddingStore::new(length, dims, Distance::Cosine);
        let var = EmbeddingStore::new(length, dims, Distance::Cosine);
        AdamOptimizer { beta_1, beta_2, mom, var, eps: 1e-8 }
    }
}

impl Optimizer for AdamOptimizer {

    fn update(
        &self, 
        feature_embeddings: &EmbeddingStore,
        grads: CHashMap<usize, Vec<f32>>,
        alpha: f32,
        t: f32
    ) {
        let t = t + 1.;
        grads.into_par_iter().for_each(|(feat_id, grad)| {

            // Can get some nans in weird cases, such as the distance between
            // a node and it's reconstruction when it shares all features.
            // We just skip over those weird ones.
            if grad.par_iter().all(|gi| !gi.is_nan()) {

                // Update first order mean
                let mom = self.mom.get_embedding_mut_hogwild(feat_id);
                mom.iter_mut().zip(grad.iter()).for_each(|(m_i, g_i)| {
                    *m_i = self.beta_1 * *m_i + (1. - self.beta_1) * g_i;
                });

                // Update secord order variance 
                let var = self.var.get_embedding_mut_hogwild(feat_id);
                var.iter_mut().zip(grad.iter()).for_each(|(v_i, g_i)| {
                    *v_i = self.beta_2 * *v_i + (1. - self.beta_2) * g_i.powf(2.);
                });

                // Create the new grad and update
                let emb = feature_embeddings.get_embedding_mut_hogwild(feat_id);
                emb.iter_mut().zip(mom.iter().zip(var.iter())).for_each(|(e_i, (m_i, v_i))| {
                    let m_i = m_i / (1. - self.beta_1.powf(t));
                    let v_i = v_i / (1. - self.beta_2.powf(t));
                    let g_i = m_i / (v_i.sqrt() + self.eps);
                    *e_i -= alpha * g_i;
                });

            }
        });
    }
}

