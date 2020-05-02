#[derive(PartialEq, Debug)]
enum Condition {
    TagPresence(&'static str),
    ValueMatch(&'static str, &'static str),
}

#[derive(PartialEq, Debug)]
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

pub fn filter(groups: &Vec<Group>) -> bool {
    unimplemented!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single() {
        let condition_1 = Condition::TagPresence("amenity");

        let group = Group {
            conditions: vec![condition_1],
        };
        assert_eq!(parse("amenity"), vec![group]);
    }

    #[test]
    fn parse_multiple_group() {
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

        let group = Group {
            conditions: vec![condition_1, condition_2],
        };
        assert_eq!(parse("amenity+highway"), vec![group]);
    }

    #[test]
    fn parse_value_match() {
        let condition = Condition::ValueMatch("amenity", "theatre");
        let group = Group {
            conditions: vec![condition],
        };
        assert_eq!(parse("amenity~theatre"), vec![group]);
    }
}
