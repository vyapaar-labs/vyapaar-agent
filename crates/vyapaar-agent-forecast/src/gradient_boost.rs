//! Gradient-boosted regression trees.
//!
//! A from-scratch, deterministic gradient boosting machine: shallow CART
//! regression trees trained on the negative gradient of the squared-loss
//! objective with a learning-rate shrinkage. No external ML dependency, no
//! randomness — split selection scans all feature thresholds exhaustively, so
//! fitting the same series twice yields the same model byte-for-byte.

use crate::error::ForecastError;

/// Node of a regression tree.
#[derive(Debug, Clone)]
enum Node {
    /// Leaf carrying a constant prediction.
    Leaf { value: f64 },
    /// Binary split on `features[feature] <= threshold`.
    Split {
        feature: usize,
        threshold: f64,
        left: Box<Node>,
        right: Box<Node>,
    },
}

/// Configuration for regression tree construction.
#[derive(Debug, Clone, Copy)]
pub struct RegressionTreeConfig {
    /// Maximum tree depth.
    pub max_depth: usize,
    /// Minimum leaf size enforced when creating splits.
    pub min_samples_leaf: usize,
}

impl Default for RegressionTreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            min_samples_leaf: 2,
        }
    }
}

/// A regression tree over fixed-length feature vectors.
#[derive(Debug, Clone)]
pub struct RegressionTree {
    root: Node,
}

impl RegressionTree {
    /// Fit a regression tree minimizing squared error.
    pub fn fit(
        x: &[Vec<f64>],
        y: &[f64],
        config: &RegressionTreeConfig,
    ) -> Result<Self, ForecastError> {
        if x.is_empty() || x.len() != y.len() {
            return Err(ForecastError::InsufficientData {
                needed: 1,
                got: x.len(),
            });
        }
        if x.iter().any(|row| row.len() != x[0].len()) {
            return Err(ForecastError::InvalidConfig(
                "feature rows have inconsistent lengths".to_string(),
            ));
        }
        let indices: Vec<usize> = (0..x.len()).collect();
        let root = Self::build_node(x, y, &indices, 0, config);
        Ok(Self { root })
    }

    /// Predict the output for one feature vector.
    pub fn predict(&self, features: &[f64]) -> f64 {
        let mut node = &self.root;
        loop {
            match node {
                Node::Leaf { value } => return *value,
                Node::Split {
                    feature,
                    threshold,
                    left,
                    right,
                } => {
                    if features[*feature] <= *threshold {
                        node = left;
                    } else {
                        node = right;
                    }
                }
            }
        }
    }

    fn build_node(
        x: &[Vec<f64>],
        y: &[f64],
        indices: &[usize],
        depth: usize,
        config: &RegressionTreeConfig,
    ) -> Node {
        let value = indices.iter().map(|&i| y[i]).sum::<f64>() / indices.len() as f64;
        if indices.len() < 2 * config.min_samples_leaf || depth >= config.max_depth {
            return Node::Leaf { value };
        }

        let mut best: Option<(usize, f64, Vec<usize>, Vec<usize>)> = None; // feature, threshold, left, right
        for feature in 0..x[0].len() {
            if let Some((threshold, _, left, right)) = Self::best_split(x, y, indices, feature) {
                let better = match &best {
                    None => true,
                    Some((_, _, bl, br)) => split_sse(y, &left, &right) < split_sse(y, bl, br),
                };
                if better {
                    best = Some((feature, threshold, left, right));
                }
            }
        }

        let Some((feature, threshold, left, right)) = best else {
            return Node::Leaf { value };
        };
        if left.len() < config.min_samples_leaf || right.len() < config.min_samples_leaf {
            return Node::Leaf { value };
        }

        Node::Split {
            feature,
            threshold,
            left: Box::new(Self::build_node(x, y, &left, depth + 1, config)),
            right: Box::new(Self::build_node(x, y, &right, depth + 1, config)),
        }
    }

    /// Find the variance-reducing threshold on one feature.
    ///
    /// Returns `(threshold, improvement, left_indices, right_indices)` or
    /// `None` when no split improves the squared error.
    fn best_split(
        x: &[Vec<f64>],
        y: &[f64],
        indices: &[usize],
        feature: usize,
    ) -> Option<(f64, f64, Vec<usize>, Vec<usize>)> {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_by(|&a, &b| {
            x[a][feature]
                .partial_cmp(&x[b][feature])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let n = sorted.len();
        let total_sum: f64 = sorted.iter().map(|&i| y[i]).sum();
        let total_sq: f64 = sorted.iter().map(|&i| y[i] * y[i]).sum();
        let total_sse = total_sq - total_sum * total_sum / n as f64;

        let mut left_sum = 0.0;
        let mut left_sq = 0.0;
        let mut best: Option<(f64, f64, Vec<usize>, Vec<usize>)> = None;

        for split in 0..n - 1 {
            let idx = sorted[split];
            left_sum += y[idx];
            left_sq += y[idx] * y[idx];
            let left_n = split + 1;
            let right_n = n - left_n;
            // Only split between distinct feature values.
            if (x[sorted[split]][feature] - x[sorted[split + 1]][feature]).abs() <= f64::EPSILON {
                continue;
            }
            let left_sse = left_sq - left_sum * left_sum / left_n as f64;
            let right_sum = total_sum - left_sum;
            let right_sq = total_sq - left_sq;
            let right_sse = right_sq - right_sum * right_sum / right_n as f64;
            let sse = left_sse + right_sse;
            let improvement = total_sse - sse;
            if improvement > 0.0
                && best
                    .as_ref()
                    .is_none_or(|(_, best_imp, _, _)| improvement > *best_imp)
            {
                let threshold = (x[sorted[split]][feature] + x[sorted[split + 1]][feature]) / 2.0;
                best = Some((
                    threshold,
                    improvement,
                    sorted[..left_n].to_vec(),
                    sorted[left_n..].to_vec(),
                ));
            }
        }
        best
    }
}

/// Total squared error of two disjoint index sets.
fn split_sse(y: &[f64], left: &[usize], right: &[usize]) -> f64 {
    let sse = |indices: &[usize]| {
        let sum: f64 = indices.iter().map(|&i| y[i]).sum();
        let sq: f64 = indices.iter().map(|&i| y[i] * y[i]).sum();
        if indices.is_empty() {
            0.0
        } else {
            sq - sum * sum / indices.len() as f64
        }
    };
    sse(left) + sse(right)
}

/// A gradient-boosted regression tree ensemble (squared-loss objective).
#[derive(Debug, Clone)]
pub struct GradientBoostedRegressor {
    /// Number of boosting rounds.
    pub n_estimators: usize,
    /// Shrinkage applied to each tree's contribution.
    pub learning_rate: f64,
    /// Maximum depth of each tree.
    pub max_depth: usize,
    /// Minimum leaf size per tree.
    pub min_samples_leaf: usize,
    trees: Vec<RegressionTree>,
    base_prediction: f64,
}

impl GradientBoostedRegressor {
    /// Create an untrained model with the given hyperparameters.
    pub fn new(
        n_estimators: usize,
        learning_rate: f64,
        max_depth: usize,
        min_samples_leaf: usize,
    ) -> Result<Self, ForecastError> {
        if n_estimators < 1 {
            return Err(ForecastError::InvalidConfig(
                "n_estimators must be at least 1".to_string(),
            ));
        }
        if !(learning_rate > 0.0 && learning_rate <= 1.0) {
            return Err(ForecastError::InvalidConfig(
                "learning_rate must lie in (0, 1]".to_string(),
            ));
        }
        Ok(Self {
            n_estimators,
            learning_rate,
            max_depth,
            min_samples_leaf,
            trees: Vec::new(),
            base_prediction: 0.0,
        })
    }

    /// Fit the ensemble to a supervised design matrix.
    pub fn fit(&mut self, x: &[Vec<f64>], y: &[f64]) -> Result<(), ForecastError> {
        if x.is_empty() || x.len() != y.len() {
            return Err(ForecastError::InsufficientData {
                needed: 1,
                got: x.len(),
            });
        }
        let base_prediction = y.iter().sum::<f64>() / y.len() as f64;

        let tree_config = RegressionTreeConfig {
            max_depth: self.max_depth,
            min_samples_leaf: self.min_samples_leaf,
        };

        let mut residuals: Vec<f64> = y.iter().map(|v| v - base_prediction).collect();
        let mut trees = Vec::with_capacity(self.n_estimators);
        for _ in 0..self.n_estimators {
            let tree = RegressionTree::fit(x, &residuals, &tree_config)?;
            for (i, row) in x.iter().enumerate() {
                residuals[i] -= self.learning_rate * tree.predict(row);
            }
            trees.push(tree);
        }

        self.base_prediction = base_prediction;
        self.trees = trees;
        Ok(())
    }

    /// Predict the target for one feature vector.
    pub fn predict(&self, features: &[f64]) -> f64 {
        self.base_prediction
            + self
                .trees
                .iter()
                .map(|tree| self.learning_rate * tree.predict(features))
                .sum::<f64>()
    }

    /// Predict targets for a matrix of feature vectors.
    pub fn predict_rows(&self, x: &[Vec<f64>]) -> Vec<f64> {
        x.iter().map(|row| self.predict(row)).collect()
    }
}
