//! Algoritmos base de geoda (fases 1, 4, 5 y 6). Todo en Rust seguro: ningun `unsafe`.
//!
//! Los algoritmos se escriben a mano (el proyecto es «desde cero»); la biblioteca
//! estandar solo se usa como oraculo en los tests.

#![forbid(unsafe_code)]

pub mod apriori;
pub mod components;
pub mod dedupe;
pub mod edit;
pub mod graph;
pub mod hash;
pub mod heap;
pub mod hierarchical;
pub mod huffman;
pub mod kmeans;
pub mod lis;
pub mod matrix;
pub mod network;
pub mod percentile;
pub mod search;
pub mod select;
pub mod shortest;
pub mod sort;
pub mod spanning;
pub mod text;
pub mod traverse;
pub mod tree;

pub use apriori::{
    FrequentItemsets, MAX_RULE_ITEMS, Rule, Transactions, association_rules, frequent_itemsets,
};
pub use components::{Components, UnionFind, components, topological_order};
pub use dedupe::{Dedupe, MAX_INDEXED_DISTANCE, dedupe_column, dedupe_similar};
pub use edit::{edit_distance, edit_distance_within};
pub use graph::{Graph, WeightedGraph};
pub use hash::{Grouping, HashTable, Key, Value, ValueCounts, group_column, value_counts_column};
pub use heap::{Heap, top_k, top_k_column};
pub use hierarchical::{
    Dendrogram, MAX_POINTS, cut_merges, single_linkage, single_linkage_columns,
};
pub use huffman::{Code, Encoded, Huffman, frequencies};
pub use kmeans::{KMeans, KMeansParams, kmeans, kmeans_columns};
pub use lis::{Lis, lis};
pub use matrix::{Dataset, Rng, squared_distance};
pub use network::Network;
pub use percentile::{median_column, percentile_column, percentiles_column};
pub use search::{lower_bound, lower_bound_column, upper_bound, upper_bound_column};
pub use select::{Method, Scalar, median_of_medians, quickselect, select_column};
pub use shortest::{ShortestPaths, dijkstra, reconstruct_path};
pub use sort::{merge_sort, sort_column};
pub use spanning::{SpanningForest, kruskal};
pub use text::{
    edit_distance_columns, huffman_decode_column, huffman_encode_column, huffman_from_counts,
    lis_column,
};
pub use traverse::{Bfs, Dfs, bfs, dfs};
pub use tree::{DecisionTree, Node, TreeParams, fit as fit_tree, fit_columns as fit_tree_columns};
