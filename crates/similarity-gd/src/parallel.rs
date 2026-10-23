#![allow(clippy::uninlined_format_args)]

use crate::gdscript_parser::GDScriptParser;
use rayon::prelude::*;
use similarity_core::{
    cli_parallel::SimilarityResult,
    language_parser::GenericFunctionDef,
    tree::TreeNode,
    tsed::{calculate_tsed_with_threshold, TSEDOptions},
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

/// Lightweight fingerprint: histogram of node kinds in the tree
type NodeFingerprint = HashMap<u16, u16>;

/// Pre-parsed function data with tree, size, and fingerprint
struct ParsedFunc {
    tree: Rc<TreeNode>,
    size: usize,
    fingerprint: NodeFingerprint,
}

/// Build a fingerprint from a TreeNode by counting occurrences of each node kind.
/// Uses a u16 hash of the label to keep the fingerprint compact.
fn build_fingerprint(node: &TreeNode) -> NodeFingerprint {
    let mut fp = HashMap::new();
    build_fingerprint_recursive(node, &mut fp);
    fp
}

fn build_fingerprint_recursive(node: &TreeNode, fp: &mut NodeFingerprint) {
    let key = hash_label(&node.label);
    *fp.entry(key).or_insert(0) += 1;
    for child in &node.children {
        build_fingerprint_recursive(child, fp);
    }
}

fn hash_label(label: &str) -> u16 {
    let mut h: u32 = 0;
    for b in label.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    h as u16
}

/// Quick Jaccard-like similarity between two fingerprints.
/// Returns a value between 0.0 and 1.0.
fn fingerprint_similarity(a: &NodeFingerprint, b: &NodeFingerprint) -> f64 {
    let mut intersection: u32 = 0;
    let mut union: u32 = 0;

    for (key, &count_a) in a {
        let count_b = b.get(key).copied().unwrap_or(0);
        intersection += count_a.min(count_b) as u32;
        union += count_a.max(count_b) as u32;
    }
    for (key, &count_b) in b {
        if !a.contains_key(key) {
            union += count_b as u32;
        }
    }

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Check for duplicates within GDScript files in parallel
pub fn check_within_file_duplicates_parallel(
    files: &[PathBuf],
    threshold: f64,
    options: &TSEDOptions,
) -> Vec<(PathBuf, Vec<SimilarityResult<GenericFunctionDef>>)> {
    files
        .par_iter()
        .filter_map(|file| {
            let code = fs::read_to_string(file).ok()?;
            let mut parser = GDScriptParser::new().ok()?;
            let functions = parser.extract_function_trees(&code).ok()?;

            let parsed: Vec<ParsedFunc> = functions
                .iter()
                .map(|(_, tree)| ParsedFunc {
                    tree: tree.clone(),
                    size: tree.get_subtree_size(),
                    fingerprint: build_fingerprint(tree),
                })
                .collect();

            let mut similar_pairs = Vec::new();

            for i in 0..functions.len() {
                for j in (i + 1)..functions.len() {
                    let func1 = &functions[i].0;
                    let func2 = &functions[j].0;

                    if func1.end_line - func1.start_line + 1 < options.min_lines
                        || func2.end_line - func2.start_line + 1 < options.min_lines
                    {
                        continue;
                    }

                    let (p1, p2) = (&parsed[i], &parsed[j]);

                    if let Some(min_tokens) = options.min_tokens {
                        if (p1.size as u32) < min_tokens || (p2.size as u32) < min_tokens {
                            continue;
                        }
                    }

                    // Pre-filter 1: tree size ratio
                    let (min_s, max_s) =
                        if p1.size <= p2.size { (p1.size, p2.size) } else { (p2.size, p1.size) };
                    if max_s == 0 || (min_s as f64 / max_s as f64) < threshold {
                        continue;
                    }
                    // Pre-filter 2: node kind histogram
                    if fingerprint_similarity(&p1.fingerprint, &p2.fingerprint) < threshold {
                        continue;
                    }

                    let similarity =
                        calculate_tsed_with_threshold(&p1.tree, &p2.tree, options, threshold);
                    if similarity >= threshold {
                        similar_pairs.push(SimilarityResult::new(
                            func1.clone(),
                            func2.clone(),
                            similarity,
                        ));
                    }
                }
            }

            if similar_pairs.is_empty() {
                None
            } else {
                Some((file.clone(), similar_pairs))
            }
        })
        .collect()
}
