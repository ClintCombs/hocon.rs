use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;

use super::{Hocon, HoconLoaderConfig};

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct HoconInternal {
    pub(crate) internal: Hash,
}

impl HoconInternal {
    pub(crate) fn empty() -> Self {
        Self { internal: vec![] }
    }

    pub(crate) fn add(&self, other: HoconInternal) -> Self {
        let mut elems = self.internal.clone();
        elems.append(&mut other.internal.clone());
        Self { internal: elems }
    }

    pub(crate) fn from_properties(properties: HashMap<String, String>) -> Self {
        Self {
            internal: properties
                .into_iter()
                .map(|(path, value)| {
                    (
                        path.split('.')
                            .map(|s| HoconValue::String(String::from(s)))
                            .collect(),
                        HoconValue::String(value),
                    )
                })
                .collect(),
        }
    }

    pub(crate) fn from_value(v: HoconValue) -> Self {
        Self {
            internal: vec![(vec![], v)],
        }
    }

    pub(crate) fn from_object(h: Hash) -> Self {
        if h.is_empty() {
            Self {
                internal: vec![(vec![], HoconValue::EmptyObject)],
            }
        } else {
            Self { internal: h }
        }
    }

    pub(crate) fn from_array(a: Vec<HoconInternal>) -> Self {
        if a.is_empty() {
            Self {
                internal: vec![(vec![], HoconValue::EmptyArray)],
            }
        } else {
            Self {
                internal: a
                    .into_iter()
                    .enumerate()
                    .flat_map(|(i, hw)| {
                        Self {
                            internal: hw.internal,
                        }
                        .add_to_path(vec![HoconValue::Integer(i as i64)])
                        .internal
                        .into_iter()
                    })
                    .collect(),
            }
        }
    }

    pub(crate) fn from_include(file_path: &str, config: &HoconLoaderConfig) -> Self {
        if config.include_depth > 10 || config.file_meta.is_none() {
            Self {
                internal: vec![(
                    vec![HoconValue::String(String::from(file_path))],
                    HoconValue::BadValue,
                )],
            }
        } else if let Ok(included) = {
            let include_config = config
                .included_from()
                .with_file(std::path::Path::new(file_path).to_path_buf());
            include_config
                .read_file()
                .and_then(|s| include_config.parse_str_to_internal(s))
        } {
            Self {
                internal: included
                    .internal
                    .into_iter()
                    .map(|(path, value)| {
                        (
                            path.clone(),
                            HoconValue::Included {
                                value: Box::new(value),
                                original_path: path,
                            },
                        )
                    })
                    .collect(),
            }
        } else {
            Self {
                internal: vec![(
                    vec![HoconValue::String(String::from(file_path))],
                    HoconValue::BadValue,
                )],
            }
        }
    }

    pub(crate) fn add_include(&mut self, file_path: &str, config: &HoconLoaderConfig) -> Self {
        let mut included = Self::from_include(file_path, config);

        included.internal.append(&mut self.internal);

        included
    }

    pub(crate) fn add_to_path(self, p: Path) -> Self {
        Self {
            internal: self
                .internal
                .into_iter()
                .map(|(mut k, v)| {
                    let mut new_path = p.clone();
                    new_path.append(&mut k);
                    (new_path, v)
                })
                .collect(),
        }
    }

    pub(crate) fn merge(self) -> Result<HoconIntermediate, ()> {
        let root = Rc::new(Child {
            key: HoconValue::BadValue,
            value: RefCell::new(Node::Node {
                children: vec![],
                key_hint: None,
            }),
        });

        for (path, item) in self.internal {
            let mut current_node = Rc::clone(&root);

            for path_item in path.clone() {
                for path_item in match path_item {
                    HoconValue::UnquotedString(s) => s
                        .trim()
                        .split('.')
                        .map(|s| HoconValue::String(String::from(s)))
                        .collect(),
                    _ => vec![path_item],
                } {
                    let (target_child, child_list) = match current_node.value.borrow().deref() {
                        Node::Leaf(_) => {
                            let new_child = Rc::new(Child {
                                key: path_item.clone(),
                                value: RefCell::new(Node::Leaf(HoconValue::BadValue)),
                            });

                            (Rc::clone(&new_child), vec![Rc::clone(&new_child)])
                        }
                        Node::Node { children, .. } => {
                            let exist = children.iter().find(|child| child.key == path_item);
                            match exist {
                                Some(child) => (Rc::clone(child), children.clone()),
                                None => {
                                    let new_child = Rc::new(Child {
                                        key: path_item.clone(),
                                        value: RefCell::new(Node::Leaf(HoconValue::BadValue)),
                                    });
                                    let mut new_children = if children.is_empty() {
                                        children.clone()
                                    } else {
                                        match (
                                            Rc::deref(children.iter().next().unwrap()),
                                            path_item,
                                        ) {
                                            (_, HoconValue::Integer(0)) => vec![],
                                            (
                                                Child {
                                                    key: HoconValue::Integer(_),
                                                    ..
                                                },
                                                HoconValue::String(_),
                                            ) => vec![],
                                            (
                                                Child {
                                                    key: HoconValue::String(_),
                                                    ..
                                                },
                                                HoconValue::Integer(_),
                                            ) => vec![],
                                            _ => children.clone(),
                                        }
                                    };

                                    new_children.push(Rc::clone(&new_child));
                                    (new_child, new_children)
                                }
                            }
                        }
                    };
                    current_node.value.replace(Node::Node {
                        children: child_list,
                        key_hint: None,
                    });

                    current_node = target_child;
                }
            }
            let mut leaf = current_node.value.borrow_mut();
            *leaf = item.substitute(&root, &path);
        }

        Ok(HoconIntermediate {
            tree: Rc::try_unwrap(root).unwrap().value.into_inner(),
        })
    }
}

pub(crate) type Path = Vec<HoconValue>;
pub(crate) type Hash = Vec<(Path, HoconValue)>;

#[derive(Clone, Debug)]
enum KeyType {
    Int,
    String,
}

#[derive(Clone, Debug)]
enum Node {
    Leaf(HoconValue),
    Node {
        children: Vec<Rc<Child>>,
        key_hint: Option<KeyType>,
    },
}

impl Node {
    fn deep_clone(&self) -> Self {
        match self {
            Node::Leaf(v) => Node::Leaf(v.clone()),
            Node::Node { children, key_hint } => Node::Node {
                children: children.iter().map(|v| Rc::new(v.deep_clone())).collect(),
                key_hint: key_hint.clone(),
            },
        }
    }

    fn deep_clone_and_update_include_path(&self, to_path: &[HoconValue]) -> Self {
        match self {
            Node::Leaf(v) => match v {
                HoconValue::Included { value, .. } => Node::Leaf(HoconValue::Included {
                    value: value.clone(),
                    original_path: to_path.to_vec(),
                }),
                HoconValue::Concat(values) => Node::Leaf(HoconValue::Concat(
                    values
                        .iter()
                        .map(|value| match value {
                            HoconValue::Included { value, .. } => HoconValue::Included {
                                value: value.clone(),
                                original_path: to_path.to_vec(),
                            },
                            _ => value.clone(),
                        })
                        .collect(),
                )),
                _ => Node::Leaf(v.clone()),
            },
            Node::Node { children, key_hint } => Node::Node {
                children: children
                    .iter()
                    .map(|v| Rc::new(v.deep_clone_and_update_include_path(to_path)))
                    .collect(),
                key_hint: key_hint.clone(),
            },
        }
    }

    fn finalize(
        self,
        root: &HoconIntermediate,
        config: &HoconLoaderConfig,
        included_path: Option<Vec<HoconValue>>,
        at_path: &[HoconValue],
    ) -> Hocon {
        match self {
            Node::Leaf(v) => v.finalize(root, config, false, included_path, at_path),
            Node::Node {
                ref children,
                ref key_hint,
            } => children
                .first()
                .map(|ref first| match first.key {
                    HoconValue::Integer(_) => Hocon::Array(
                        children
                            .iter()
                            .map(|c| {
                                let mut new_path = at_path.to_vec();
                                new_path.push(c.key.clone());
                                c.value.clone().into_inner().finalize(
                                    root,
                                    config,
                                    included_path.clone(),
                                    &new_path,
                                )
                            })
                            .collect(),
                    ),
                    HoconValue::String(_) => Hocon::Hash(
                        children
                            .iter()
                            .map(|c| {
                                let mut new_path = at_path.to_vec();
                                new_path.push(c.key.clone());
                                (
                                    c.key.clone().string_value(),
                                    c.value.clone().into_inner().finalize(
                                        root,
                                        config,
                                        included_path.clone(),
                                        &new_path,
                                    ),
                                )
                            })
                            .collect(),
                    ),
                    // Keys should only be integer or strings
                    _ => unreachable!(),
                })
                .unwrap_or_else(|| match key_hint {
                    Some(KeyType::Int) => Hocon::Array(vec![]),
                    Some(KeyType::String) | None => Hocon::Hash(HashMap::new()),
                }),
        }
    }

    fn find_key(&self, path: Vec<HoconValue>) -> Node {
        match (self, &path) {
            (Node::Leaf(_), ref path) if path.is_empty() => self.clone(),
            (Node::Node { children, .. }, _) => {
                let mut iter = path.clone().into_iter();
                let first = iter.nth(0);
                let remaining = iter.collect();

                match first {
                    None => self.clone(),
                    Some(first) => children
                        .iter()
                        .find(|child| child.key == first)
                        .map(|child| child.find_key(remaining))
                        .unwrap_or(Node::Leaf(HoconValue::BadValue)),
                }
            }
            _ => Node::Leaf(HoconValue::BadValue),
        }
    }
}

#[derive(Debug)]
struct Child {
    key: HoconValue,
    value: RefCell<Node>,
}

impl Child {
    fn find_key(&self, path: Vec<HoconValue>) -> Node {
        self.value.clone().into_inner().find_key(path)
    }

    fn deep_clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: RefCell::new(self.value.clone().into_inner().deep_clone()),
        }
    }

    fn deep_clone_and_update_include_path(&self, to_path: &[HoconValue]) -> Self {
        Self {
            key: self.key.clone(),
            value: RefCell::new(
                self.value
                    .clone()
                    .into_inner()
                    .deep_clone_and_update_include_path(to_path),
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HoconIntermediate {
    tree: Node,
}

impl HoconIntermediate {
    pub(crate) fn finalize(self, config: &HoconLoaderConfig) -> Hocon {
        let refself = &self.clone();
        self.tree.finalize(refself, config, None, &[])
    }
}

#[derive(Clone, Debug)]
pub(crate) enum HoconValue {
    Real(f64),
    Integer(i64),
    String(String),
    UnquotedString(String),
    Boolean(bool),
    Concat(Vec<HoconValue>),
    PathSubstitution(Box<HoconValue>),
    Null,
    BadValue,
    EmptyObject,
    EmptyArray,
    Included {
        value: Box<HoconValue>,
        original_path: Vec<HoconValue>,
    },
}

impl HoconValue {
    pub(crate) fn maybe_concat(values: Vec<HoconValue>) -> HoconValue {
        let nb_values = values.len();
        let trimmed_values: Vec<HoconValue> = values
            .into_iter()
            .enumerate()
            .filter_map(|item| match item {
                (0, HoconValue::UnquotedString(ref s)) if s.trim() == "" => None,
                (i, HoconValue::UnquotedString(ref s)) if s.trim() == "" && i == nb_values - 1 => {
                    None
                }
                (_, v) => Some(v),
            })
            .collect();
        match trimmed_values {
            ref values if values.len() == 1 => values.first().unwrap().clone(),
            values => HoconValue::Concat(values),
        }
    }

    fn to_path(&self) -> Vec<HoconValue> {
        match self {
            HoconValue::UnquotedString(s) if s == "." => vec![],
            HoconValue::UnquotedString(s) => s
                .trim()
                .split('.')
                .map(String::from)
                .map(HoconValue::String)
                .collect(),
            HoconValue::String(s) => vec![HoconValue::String(s.clone())],
            HoconValue::Concat(values) => values.iter().flat_map(HoconValue::to_path).collect(),
            _ => vec![self.clone()],
        }
    }

    fn finalize(
        self,
        root: &HoconIntermediate,
        config: &HoconLoaderConfig,
        in_concat: bool,
        included_path: Option<Vec<HoconValue>>,
        at_path: &[HoconValue],
    ) -> Hocon {
        match self {
            HoconValue::Null => Hocon::Null,
            HoconValue::BadValue => Hocon::BadValue,
            HoconValue::Boolean(b) => Hocon::Boolean(b),
            HoconValue::Integer(i) => Hocon::Integer(i),
            HoconValue::Real(f) => Hocon::Real(f),
            HoconValue::String(s) => Hocon::String(s),
            HoconValue::UnquotedString(ref s) if s == "null" => Hocon::Null,
            HoconValue::UnquotedString(s) => {
                if in_concat {
                    Hocon::String(s)
                } else {
                    Hocon::String(String::from(s.trim()))
                }
            }
            HoconValue::Concat(values) => Hocon::String({
                let nb_items = values.len();
                values
                    .into_iter()
                    .enumerate()
                    .map(|item| match item {
                        (0, HoconValue::UnquotedString(s)) => {
                            HoconValue::UnquotedString(String::from(s.trim_start()))
                        }
                        (i, HoconValue::UnquotedString(ref s)) if i == nb_items - 1 => {
                            HoconValue::UnquotedString(String::from(s.trim_end()))
                        }
                        (_, v) => v,
                    })
                    .map(|v| v.finalize(root, config, true, included_path.clone(), at_path))
                    .filter_map(|v| v.as_internal_string())
                    .collect::<Vec<String>>()
                    .join("")
            }),
            HoconValue::PathSubstitution(v) => {
                // second pass for substitution
                let fixed_up_path = if let Some(included_path) = included_path.clone() {
                    let mut fixed_up_path = at_path
                        .iter()
                        .take(at_path.len() - included_path.len())
                        .cloned()
                        .flat_map(|path_item| path_item.to_path())
                        .collect::<Vec<_>>();
                    fixed_up_path.append(&mut v.to_path());
                    fixed_up_path
                } else {
                    v.to_path()
                };
                match (
                    config.system,
                    root.tree.find_key(fixed_up_path.clone()).finalize(
                        root,
                        config,
                        included_path,
                        at_path,
                    ),
                ) {
                    (true, Hocon::BadValue) => {
                        match std::env::var(
                            v.to_path()
                                .into_iter()
                                .map(HoconValue::string_value)
                                .collect::<Vec<_>>()
                                .join("."),
                        ) {
                            Ok(val) => Hocon::String(val),
                            Err(_) => Hocon::BadValue,
                        }
                    }
                    (_, v) => v,
                }
            }
            HoconValue::Included {
                value,
                original_path,
            } => value
                .clone()
                .finalize(root, config, in_concat, Some(original_path), at_path),
            // This cases should have been replaced during substitution
            // and not exist anymore at this point
            HoconValue::EmptyObject => unreachable!(),
            HoconValue::EmptyArray => unreachable!(),
        }
    }

    fn string_value(self) -> String {
        match self {
            HoconValue::String(s) => s,
            HoconValue::Null => String::from("null"),
            _ => unreachable!(),
        }
    }

    fn substitute(self, current_tree: &Rc<Child>, at_path: &[HoconValue]) -> Node {
        match self {
            HoconValue::PathSubstitution(path) => {
                match current_tree.find_key(path.to_path()).deep_clone() {
                    Node::Leaf(HoconValue::BadValue) => {
                        // If node is not found, keep substitution to try again on second pass
                        Node::Leaf(HoconValue::PathSubstitution(path))
                    }
                    v => v,
                }
            }
            HoconValue::Concat(values) => Node::Leaf(HoconValue::Concat(
                values
                    .into_iter()
                    .map(|v| v.substitute(&current_tree, at_path))
                    .map(|v| {
                        if let Node::Leaf(value) = v {
                            value
                        } else {
                            HoconValue::BadValue
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            HoconValue::EmptyObject => Node::Node {
                children: vec![],
                key_hint: Some(KeyType::String),
            },
            HoconValue::EmptyArray => Node::Node {
                children: vec![],
                key_hint: Some(KeyType::Int),
            },
            HoconValue::Included {
                value,
                original_path,
            } => {
                match *value.clone() {
                    HoconValue::PathSubstitution(path) => {
                        let mut fixed_up_path = at_path
                            .iter()
                            .take(at_path.len() - original_path.len())
                            .cloned()
                            .flat_map(|path_item| path_item.to_path())
                            .collect::<Vec<_>>();
                        fixed_up_path.append(&mut path.to_path());
                        match current_tree.find_key(fixed_up_path.clone()) {
                            Node::Leaf(HoconValue::BadValue) => (),
                            new_value => {
                                return new_value.deep_clone_and_update_include_path(at_path);
                            }
                        }
                    }
                    HoconValue::Concat(values) => {
                        return HoconValue::Concat(
                            values
                                .into_iter()
                                .map(|value| HoconValue::Included {
                                    value: Box::new(value),
                                    original_path: original_path.clone(),
                                })
                                .collect(),
                        )
                        .substitute(current_tree, &at_path);
                    }
                    _ => (),
                }

                match value.clone().substitute(current_tree, &at_path) {
                    Node::Leaf(value_found) => {
                        // remember leaf was found inside an include
                        Node::Leaf(HoconValue::Included {
                            value: Box::new(value_found),
                            original_path,
                        })
                    }
                    v => v,
                }
            }
            v => Node::Leaf(v),
        }
    }
}

impl PartialEq for HoconValue {
    fn eq(&self, rhs: &Self) -> bool {
        match (self, rhs) {
            (HoconValue::Integer(left), HoconValue::Integer(right)) => left == right,
            (HoconValue::String(left), HoconValue::String(right)) => left == right,
            (HoconValue::BadValue, HoconValue::BadValue) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_depth_of_include() {
        let val = dbg!(HoconInternal::from_include(
            "file.conf",
            &HoconLoaderConfig {
                include_depth: 15,
                file_meta: Some(crate::ConfFileMeta::from_path(
                    std::path::Path::new("file.conf").to_path_buf()
                )),
                ..Default::default()
            }
        ));
        assert_eq!(
            val,
            HoconInternal {
                internal: vec![(
                    vec![HoconValue::String(String::from("file.conf"))],
                    HoconValue::BadValue
                )]
            }
        );
    }

    #[test]
    fn missing_file_included() {
        let val = dbg!(HoconInternal::from_include(
            "file.conf",
            &HoconLoaderConfig {
                include_depth: 5,
                file_meta: Some(crate::ConfFileMeta::from_path(
                    std::path::Path::new("file.conf").to_path_buf()
                )),
                ..Default::default()
            }
        ));
        assert_eq!(
            val,
            HoconInternal {
                internal: vec![(
                    vec![HoconValue::String(String::from("file.conf"))],
                    HoconValue::BadValue
                )]
            }
        );
    }

}
