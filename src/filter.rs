use osmpbfreader::objects::{Node, NodeId, OsmObj, Tags};

#[derive(PartialEq, Debug, Clone)]
enum Condition {
    TagPresence(&'static str),
    ValueMatch(&'static str, &'static str),
}

#[derive(PartialEq, Debug, Clone)]
pub struct Group {
    conditions: Vec<Condition>,
}

fn parse_condition(condition_str: &'static str) -> Condition {
    let split_str: Vec<&str> = condition_str.splitn(2, '~').collect();
    if split_str.len() < 2 {
        Condition::TagPresence(condition_str)
    } else {
        let key = split_str[0];
        let value = split_str[1];
        Condition::ValueMatch(key, value)
    }
}

fn parse_group(group_str: &'static str) -> Group {
    let condition_strs: Vec<&str> = group_str.split('+').collect();
    let conditions = condition_strs.into_iter().map(parse_condition).collect();
    Group { conditions }
}

pub fn parse(selector_str: &'static str) -> Vec<Group> {
    let group_strs: Vec<&str> = selector_str.split(',').collect();
    group_strs.into_iter().map(parse_group).collect()
}

fn check_condition(tags: &Tags, condition: &Condition) -> bool {
    match condition {
        Condition::TagPresence(key) => tags.contains_key(*key),
        Condition::ValueMatch(key, value) => tags.contains(key, value),
    }
}

fn check_group(tags: &Tags, group: &Group) -> bool {
    group.conditions.iter().all(|c| check_condition(tags, c))
}

pub fn filter(obj: &OsmObj, groups: &Vec<Group>) -> bool {
    let tags = obj.tags();
    groups.iter().any(|c| check_group(tags, c))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let condition = Condition::TagPresence("amenity");
        let conditions = vec![condition];
        let group = Group { conditions };

        let node = new_node();
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &vec![group.clone()]), false);

        let mut node = new_node();
        node.tags
            .insert("amenity".to_string(), "theatre".to_string());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &vec![group.clone()]), true);
    }

    #[test]
    fn filter_value_match() {
        let condition = Condition::ValueMatch("amenity", "theatre");
        let conditions = vec![condition];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags
            .insert("amenity".to_string(), "theatre".to_string());
        let obj = OsmObj::Node(node);
        assert_eq!(filter(&obj, &vec![group.clone()]), true);

        let mut node = new_node();
        node.tags
            .insert("amenity".to_string(), "cinema".to_string());
        let obj = OsmObj::Node(node);
        assert_eq!(filter(&obj, &vec![group.clone()]), false);
    }

    #[test]
    fn filter_multiple_groups() {
        let condition = Condition::TagPresence("amenity");
        let conditions = vec![condition];
        let group_1 = Group { conditions };
        let condition = Condition::TagPresence("architect");
        let conditions = vec![condition];
        let group_2 = Group { conditions };

        let mut node = new_node();
        node.tags
            .insert("amenity".to_string(), "theatre".to_string());
        node.tags
            .insert("name".to_string(), "Waldbühne".to_string());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &vec![group_1, group_2]), true);
    }

    #[test]
    fn filter_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity");
        let condition_2 = Condition::TagPresence("name");
        let condition_3 = Condition::TagPresence("architect");
        let conditions = vec![condition_1, condition_2.clone()];
        let group = Group { conditions };

        let mut node = new_node();
        node.tags
            .insert("amenity".to_string(), "theatre".to_string());
        node.tags
            .insert("name".to_string(), "Waldbühne".to_string());
        let obj = OsmObj::Node(node);

        assert_eq!(filter(&obj, &vec![group]), true);

        let conditions = vec![condition_2.clone(), condition_3];
        let group = Group { conditions };

        assert_eq!(filter(&obj, &vec![group]), false);
    }

    #[test]
    fn parse_single_group() {
        let condition = Condition::TagPresence("amenity");
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity"), vec![group]);
    }

    #[test]
    fn parse_multiple_groups() {
        let condition_1 = Condition::TagPresence("amenity");
        let condition_2 = Condition::TagPresence("highway");
        let group_1 = Group {
            conditions: vec![condition_1],
        };
        let group_2 = Group {
            conditions: vec![condition_2],
        };

        assert_eq!(parse("amenity,highway"), vec![group_1, group_2]);
    }

    #[test]
    fn parse_multiple_conditions() {
        let condition_1 = Condition::TagPresence("amenity");
        let condition_2 = Condition::TagPresence("highway");
        let conditions = vec![condition_1, condition_2];
        let group = Group { conditions };

        assert_eq!(parse("amenity+highway"), vec![group]);
    }

    #[test]
    fn parse_value_match() {
        let condition = Condition::ValueMatch("amenity", "theatre");
        let conditions = vec![condition];
        let group = Group { conditions };

        assert_eq!(parse("amenity~theatre"), vec![group]);
    }
}
