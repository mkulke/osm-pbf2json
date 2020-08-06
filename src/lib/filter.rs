use osmpbfreader::objects::{OsmObj, Tags};
use smartstring::alias::String;

#[derive(PartialEq, Debug, Clone)]
pub enum Condition {
    TagPresence(String),
    ValueMatch(String, String),
}

impl Condition {
    pub fn new(tag: &str, value: Option<&str>) -> Self {
        if let Some(value) = value {
            return Condition::ValueMatch(tag.into(), value.into());
        }
        Condition::TagPresence(tag.into())
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Group {
    pub conditions: Vec<Condition>,
}

fn parse_condition(condition_str: &str) -> Condition {
    let split_str: Vec<&str> = condition_str.splitn(2, '~').collect();
    if split_str.len() < 2 {
        Condition::TagPresence(condition_str.into())
    } else {
        let key = split_str[0];
        let value = split_str[1];
        Condition::ValueMatch(key.into(), value.into())
    }
}

fn parse_group(group_str: &str) -> Group {
    let condition_strs: Vec<&str> = group_str.split('+').collect();
    let conditions = condition_strs.into_iter().map(parse_condition).collect();
    Group { conditions }
}

/// Parse an expression into a filter groups
///
/// Stating a key (`amenity`), will pick all entities which are tagged using that key.
/// To further narrow down the results, a specific value can be given using a `~` field
/// separator (`amenity~fountain`). To check the presence of multiple tags for the same
/// entity, statements can be combined using the `+` operator (`'amenity~fountain+tourism'`).
/// Finally, options can be specified by concatenating groups of statements with `,`
/// (`amenity~fountain+tourism,amenity~townhall`). If an entity matches the criteria of
/// either group it will be included in the output.
///
/// # Example
///
/// ```
/// use osm_pbf2json::filter::parse;
///
/// let groups = parse("amenity~fountain+tourism,amenity~townhall".into());
/// assert_eq!(groups.len(), 2);
/// let group = &groups[0];
/// assert_eq!(group.conditions.len(), 2);
/// ```
pub fn parse(selector_str: &str) -> Vec<Group> {
    let group_strs: Vec<&str> = selector_str.split(',').collect();
    group_strs.into_iter().map(parse_group).collect()
}

fn check_condition(tags: &Tags, condition: &Condition) -> bool {
    match condition {
        Condition::TagPresence(key) => tags.contains_key(key.as_str()),
        Condition::ValueMatch(key, value) => tags.contains(key, value),
    }
}

fn check_group(tags: &Tags, group: &Group) -> bool {
    group.conditions.iter().all(|c| check_condition(tags, c))
}

pub trait Filter {
    fn filter(&self, groups: &[Group]) -> bool;
}

impl Filter for OsmObj {
    fn filter(&self, groups: &[Group]) -> bool {
        let tags = self.tags();
        groups.iter().any(|c| check_group(tags, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osmpbfreader::objects::{Node, NodeId};

    fn new_node() -> Node {
        let tags = Tags::new();
        let id = NodeId(1);
        Node {
            id,
            tags,
            decimicro_lat: 0,
            decimicro_lon: 0,
        }
    }

    #[test]
    fn filter_single_group() {
        let condition = Condition::TagPresence("amenity".into());
        let conditions = vec![condition];
        let group = Group { conditions };

        let node = new_node();
        let obj = OsmObj::Node(node);

        assert_eq!(obj.filter(&[group.clone()]), false);

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        let obj = OsmObj::Node(node);

        assert_eq!(obj.filter(&[group]), true);
    }

    #[test]
    fn filter_value_match() {
        let condition = Condition::ValueMatch("amenity".into(), "theatre".into());
        let conditions = vec![condition];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        let obj = OsmObj::Node(node);
        assert_eq!(obj.filter(&[group.clone()]), true);

        let mut node = new_node();
        node.tags.insert("amenity".into(), "cinema".into());
        let obj = OsmObj::Node(node);
        assert_eq!(obj.filter(&[group]), false);
    }

    #[test]
    fn filter_multiple_groups() {
        let condition = Condition::TagPresence("amenity".into());
        let conditions = vec![condition];
        let group_1 = Group { conditions };
        let condition = Condition::TagPresence("architect".into());
        let conditions = vec![condition];
        let group_2 = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        node.tags.insert("name".into(), "Waldbühne".into());
        let obj = OsmObj::Node(node);

        assert_eq!(obj.filter(&[group_1, group_2]), true);
    }

    #[test]
    fn filter_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity".into());
        let condition_2 = Condition::TagPresence("name".into());
        let condition_3 = Condition::TagPresence("architect".into());
        let conditions = vec![condition_1, condition_2.clone()];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        node.tags.insert("name".into(), "Waldbühne".into());
        let obj = OsmObj::Node(node);

        assert_eq!(obj.filter(&[group]), true);

        let conditions = vec![condition_2, condition_3];
        let group = Group { conditions };

        assert_eq!(obj.filter(&[group]), false);
    }

    #[test]
    fn parse_single_group() {
        let condition = Condition::TagPresence("amenity".into());
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity"), [group]);
    }

    #[test]
    fn parse_multiple_groups() {
        let condition_1 = Condition::TagPresence("amenity".into());
        let condition_2 = Condition::TagPresence("highway".into());
        let group_1 = Group {
            conditions: vec![condition_1],
        };
        let group_2 = Group {
            conditions: vec![condition_2],
        };

        assert_eq!(parse("amenity,highway"), [group_1, group_2]);
    }

    #[test]
    fn parse_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity".into());
        let condition_2 = Condition::TagPresence("highway".into());
        let conditions = vec![condition_1, condition_2];
        let group = Group { conditions };

        assert_eq!(parse("amenity+highway"), vec![group]);
    }

    #[test]
    fn parse_value_match() {
        let condition = Condition::ValueMatch("amenity".into(), "theatre".into());
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity~theatre"), vec![group]);
    }
}
