#![allow(clippy::io_other_error)]

use similarity_core::language_parser::{
    GenericFunctionDef, GenericTypeDef, Language, LanguageParser,
};
use similarity_core::tree::TreeNode;
use std::error::Error;
use std::rc::Rc;
use tree_sitter::{Node, Parser};

/// Node kinds whose source text is kept as the node value
const VALUE_NODES: &[&str] = &[
    "identifier",
    "name",
    "string",
    "integer",
    "float",
    "true",
    "false",
    "null",
    "node_path",
    "string_name",
    "get_node",
];

/// Node kinds dropped from comparison trees
const SKIP_NODES: &[&str] = &["comment", "annotation", "annotations", "region_start", "region_end"];

/// A function definition paired with its comparison tree
pub type FunctionWithTree = (GenericFunctionDef, Rc<TreeNode>);

pub struct GDScriptParser {
    parser: Parser,
}

fn boxed_err(msg: &str) -> Box<dyn Error + Send + Sync> {
    Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.to_string()))
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

impl GDScriptParser {
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .map_err(|e| boxed_err(&format!("Failed to set GDScript language: {e:?}")))?;
        Ok(Self { parser })
    }

    fn parse_tree(
        &mut self,
        source: &str,
    ) -> Result<tree_sitter::Tree, Box<dyn Error + Send + Sync>> {
        self.parser.parse(source, None).ok_or_else(|| boxed_err("Failed to parse GDScript source"))
    }

    /// Extract every function together with its comparison tree.
    ///
    /// The tree covers parameters, return type and body (not the name or
    /// annotations), so renamed copies still match.
    pub fn extract_function_trees(
        &mut self,
        source: &str,
    ) -> Result<Vec<FunctionWithTree>, Box<dyn Error + Send + Sync>> {
        let tree = self.parse_tree(source)?;
        let mut out = Vec::new();
        visit_container(tree.root_node(), source, None, &mut out);
        Ok(out)
    }
}

fn visit_container(
    container: Node,
    source: &str,
    class_name: Option<&str>,
    out: &mut Vec<FunctionWithTree>,
) {
    // Annotations on their own line (e.g. `@rpc`) apply to the next statement
    let mut pending: Vec<String> = Vec::new();
    let mut cursor = container.walk();
    for child in container.named_children(&mut cursor) {
        match child.kind() {
            "annotation" => {
                pending.push(text(child, source).to_string());
                continue;
            }
            "function_definition" | "constructor_definition" => {
                if let Some(mut f) = extract_function(child, source, class_name) {
                    f.0.decorators.splice(0..0, pending.drain(..));
                    out.push(f);
                }
            }
            "class_definition" => {
                let name = child.child_by_field_name("name").map(|n| text(n, source).to_string());
                let qualified = match (class_name, name) {
                    (Some(outer), Some(n)) => format!("{outer}.{n}"),
                    (None, Some(n)) => n,
                    (outer, None) => outer.unwrap_or("<class>").to_string(),
                };
                if let Some(body) = child.child_by_field_name("body") {
                    visit_container(body, source, Some(&qualified), out);
                }
            }
            _ => {}
        }
        pending.clear();
    }
}

fn extract_function(
    node: Node,
    source: &str,
    class_name: Option<&str>,
) -> Option<FunctionWithTree> {
    let name = if node.kind() == "constructor_definition" {
        "_init".to_string()
    } else {
        text(node.child_by_field_name("name")?, source).to_string()
    };
    let body = node.child_by_field_name("body")?;
    let params_node = node.child_by_field_name("parameters");

    let mut decorators = Vec::new();
    let mut start_row = None;
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "annotations" => {
                let mut ac = c.walk();
                decorators.extend(c.named_children(&mut ac).map(|a| text(a, source).to_string()));
            }
            "annotation" => decorators.push(text(c, source).to_string()),
            "static_keyword" => {
                decorators.push("static".to_string());
                start_row.get_or_insert(c.start_position().row);
            }
            _ => {
                start_row.get_or_insert(c.start_position().row);
            }
        }
    }

    let parameters = params_node
        .map(|p| {
            let mut pc = p.walk();
            p.named_children(&mut pc)
                .filter_map(|param| {
                    if param.kind() == "identifier" {
                        Some(param)
                    } else {
                        param.named_child(0).filter(|n| n.kind() == "identifier")
                    }
                })
                .map(|id| text(id, source).to_string())
                .collect()
        })
        .unwrap_or_default();

    // Comparison tree: parameters + return type + body
    let mut id_counter = 0;
    let mut root = TreeNode::new("function".to_string(), String::new(), id_counter);
    id_counter += 1;
    let mut size = 1;
    let parts = [params_node, node.child_by_field_name("return_type"), Some(body)];
    for part in parts.into_iter().flatten() {
        let child = convert_node(part, source, &mut id_counter);
        size += child.subtree_size.unwrap_or(1);
        root.add_child(Rc::new(child));
    }
    root.subtree_size = Some(size);

    let def = GenericFunctionDef {
        name,
        // Start at the `func` line rather than a preceding annotation line
        start_line: start_row.unwrap_or(node.start_position().row) as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
        body_start_line: body.start_position().row as u32 + 1,
        body_end_line: body.end_position().row as u32 + 1,
        parameters,
        is_method: class_name.is_some(),
        class_name: class_name.map(str::to_string),
        is_async: false,
        is_generator: false,
        decorators,
    };
    Some((def, Rc::new(root)))
}

fn convert_node(node: Node, source: &str, id_counter: &mut usize) -> TreeNode {
    let id = *id_counter;
    *id_counter += 1;
    let kind = node.kind();
    let value =
        if VALUE_NODES.contains(&kind) { text(node, source).to_string() } else { String::new() };
    let mut tree_node = TreeNode::new(kind.to_string(), value, id);
    let mut size = 1;
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if SKIP_NODES.contains(&c.kind()) {
            continue;
        }
        let child = convert_node(c, source, id_counter);
        size += child.subtree_size.unwrap_or(1);
        tree_node.add_child(Rc::new(child));
    }
    tree_node.subtree_size = Some(size);
    tree_node
}

impl LanguageParser for GDScriptParser {
    fn parse(
        &mut self,
        source: &str,
        _filename: &str,
    ) -> Result<Rc<TreeNode>, Box<dyn Error + Send + Sync>> {
        let tree = self.parse_tree(source)?;
        let mut id_counter = 0;
        Ok(Rc::new(convert_node(tree.root_node(), source, &mut id_counter)))
    }

    fn extract_functions(
        &mut self,
        source: &str,
        _filename: &str,
    ) -> Result<Vec<GenericFunctionDef>, Box<dyn Error + Send + Sync>> {
        Ok(self.extract_function_trees(source)?.into_iter().map(|(def, _)| def).collect())
    }

    fn extract_types(
        &mut self,
        source: &str,
        _filename: &str,
    ) -> Result<Vec<GenericTypeDef>, Box<dyn Error + Send + Sync>> {
        let tree = self.parse_tree(source)?;
        let mut types = Vec::new();
        collect_classes(tree.root_node(), source, &mut types);
        Ok(types)
    }

    fn language(&self) -> Language {
        Language::GDScript
    }
}

fn collect_classes(node: Node, source: &str, types: &mut Vec<GenericTypeDef>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_definition" => {
                if let Some(name) = child.child_by_field_name("name") {
                    let mut fields = Vec::new();
                    if let Some(body) = child.child_by_field_name("body") {
                        let mut bc = body.walk();
                        for member in body.named_children(&mut bc) {
                            if member.kind().ends_with("variable_statement")
                                || member.kind() == "const_statement"
                                || member.kind() == "signal_statement"
                            {
                                if let Some(n) = member.child_by_field_name("name") {
                                    fields.push(text(n, source).to_string());
                                }
                            }
                        }
                    }
                    types.push(GenericTypeDef {
                        name: text(name, source).to_string(),
                        kind: "class".to_string(),
                        start_line: child.start_position().row as u32 + 1,
                        end_line: child.end_position().row as u32 + 1,
                        fields,
                    });
                }
                collect_classes(child, source, types);
            }
            "class_body" => collect_classes(child, source, types),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"class_name Player
extends CharacterBody2D

signal died

@export var speed := 200.0
var health := 100

func _ready() -> void:
	health = 100

@rpc
func take_damage(amount: int) -> void:
	health -= amount
	if health <= 0:
		died.emit()

static func clamp_speed(v: float) -> float:
	return clamp(v, 0.0, 10.0)

class Inventory:
	var items = []

	func add(item):
		items.append(item)
"#;

    fn funcs() -> Vec<FunctionWithTree> {
        GDScriptParser::new().unwrap().extract_function_trees(SAMPLE).unwrap()
    }

    #[test]
    fn extracts_all_functions_including_inner_class_methods() {
        let names: Vec<_> = funcs().iter().map(|(d, _)| d.name.clone()).collect();
        assert_eq!(names, vec!["_ready", "take_damage", "clamp_speed", "add"]);
    }

    #[test]
    fn records_lines_params_decorators_and_class() {
        let f = funcs();
        let take = &f.iter().find(|(d, _)| d.name == "take_damage").unwrap().0;
        assert_eq!(take.parameters, vec!["amount".to_string()]);
        assert_eq!(take.decorators, vec!["@rpc".to_string()]);
        assert_eq!(take.start_line, 13, "range starts at the func line, not the annotation");
        assert_eq!(take.end_line, 16);
        let st = &f.iter().find(|(d, _)| d.name == "clamp_speed").unwrap().0;
        assert!(st.decorators.contains(&"static".to_string()));
        let add = &f.iter().find(|(d, _)| d.name == "add").unwrap().0;
        assert!(add.is_method);
        assert_eq!(add.class_name.as_deref(), Some("Inventory"));
    }

    #[test]
    fn comparison_tree_ignores_names_and_annotations() {
        let mut p = GDScriptParser::new().unwrap();
        let a =
            p.extract_function_trees("@rpc\nfunc a(x):\n\tvar y = x * 2\n\treturn y\n").unwrap();
        let b = p.extract_function_trees("func b(x):\n\tvar y = x * 2\n\treturn y\n").unwrap();
        assert_eq!(a[0].1.get_subtree_size(), b[0].1.get_subtree_size());
    }

    #[test]
    fn language_parser_trait() {
        let mut p = GDScriptParser::new().unwrap();
        assert_eq!(p.language(), Language::GDScript);
        assert_eq!(p.extract_functions(SAMPLE, "player.gd").unwrap().len(), 4);
        assert!(p.parse(SAMPLE, "player.gd").unwrap().get_subtree_size() > 10);
        let types = p.extract_types(SAMPLE, "player.gd").unwrap();
        assert_eq!(types[0].name, "Inventory");
        assert_eq!(types[0].fields, vec!["items".to_string()]);
    }
}
