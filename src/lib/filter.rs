use osmpbfreader::objects::{OsmObj, Tags};

#[derive(PartialEq, Debug, Clone)]
pub enum Condition {
    TagPresence(String),
    ValueMatch(String, String),
}

#[derive(PartialEq, Debug, Clone)]
pub struct Group {
    pub conditions: Vec<Condition>,
}

fn parse_condition(condition_str: &str) -> Condition {
    let split_str: Vec<&str> = condition_str.splitn(2, '~').collect();
    if split_str.len() < 2 {
        Condition::TagPresence(condition_str.to_string())
    } else {
        let key = split_str[0];
        let value = split_str[1];
        Condition::ValueMatch(key.to_string(), value.to_string())
    }
}

fn parse_group(group_str: &str) -> Group {
    let condition_strs: Vec<&str> = group_str.split('+').collect();
    let conditions = condition_strs.into_iter().map(parse_condition).collect();
    Group { conditions }
}

pub fn parse(selector_str: String) -> Vec<Group> {
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

pub fn filter(obj: &OsmObj, groups: &[Group]) -> bool {
    let tags = obj.tags();
    groups.iter().any(|c| check_group(tags, c))
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
        let condition = Condition::TagPresence("amenity".to_string());
        let conditions = vec![condition];
        let group = Group { conditions };

        let node = new_node();
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &[group.clone()]), false);

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &[group]), true);
    }

    #[test]
    fn filter_value_match() {
        let condition = Condition::ValueMatch("amenity".to_string(), "theatre".to_string());
        let conditions = vec![condition];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        let obj = OsmObj::Node(node);
        assert_eq!(filter(&obj, &[group.clone()]), true);

        let mut node = new_node();
        node.tags.insert("amenity".into(), "cinema".into());
        let obj = OsmObj::Node(node);
        assert_eq!(filter(&obj, &[group]), false);
    }

    #[test]
    fn filter_multiple_groups() {
        let condition = Condition::TagPresence("amenity".to_string());
        let conditions = vec![condition];
        let group_1 = Group { conditions };
        let condition = Condition::TagPresence("architect".to_string());
        let conditions = vec![condition];
        let group_2 = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        node.tags.insert("name".into(), "Waldbühne".into());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &[group_1, group_2]), true);
    }

    #[test]
    fn filter_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity".to_string());
        let condition_2 = Condition::TagPresence("name".to_string());
        let condition_3 = Condition::TagPresence("architect".to_string());
        let conditions = vec![condition_1, condition_2.clone()];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags.insert("amenity".into(), "theatre".into());
        node.tags.insert("name".into(), "Waldbühne".into());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &[group]), true);

        let conditions = vec![condition_2, condition_3];
        let group = Group { conditions };

        assert_eq!(filter(&obj, &[group]), false);
    }

    #[test]
    fn parse_single_group() {
        let condition = Condition::TagPresence("amenity".to_string());
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity".to_string()), [group]);
    }

    #[test]
    fn parse_multiple_groups() {
        let condition_1 = Condition::TagPresence("amenity".to_string());
        let condition_2 = Condition::TagPresence("highway".to_string());
        let group_1 = Group {
            conditions: vec![condition_1],
        };
        let group_2 = Group {
            conditions: vec![condition_2],
        };

        assert_eq!(parse("amenity,highway".to_string()), [group_1, group_2]);
    }

    #[test]
    fn parse_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity".to_string());
        let condition_2 = Condition::TagPresence("highway".to_string());
        let conditions = vec![condition_1, condition_2];
        let group = Group { conditions };

        assert_eq!(parse("amenity+highway".to_string()), vec![group]);
    }

    #[test]
    fn parse_value_match() {
        let condition = Condition::ValueMatch("amenity".to_string(), "theatre".to_string());
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity~theatre".to_string()), vec![group]);
    }
}
