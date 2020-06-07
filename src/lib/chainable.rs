pub trait Chainable<T> {
    fn merge(&mut self);
}

#[derive(PartialEq, Debug)]
enum Connection<T> {
    Tail(T),
    Head(T),
    ReverseTail(T),
    ReverseHead(T),
}

trait Prependable<T> {
    fn prepend(&mut self, other: &[T]);
    fn reverse_prepend(&mut self, other: &[T]);
    fn reverse_extend(&mut self, other: &[T]);
}

impl<T: Copy> Prependable<T> for Vec<T> {
    fn prepend(&mut self, other: &[T]) {
        for element in other.iter().rev() {
            self.insert(0, *element);
        }
    }

    fn reverse_prepend(&mut self, other: &[T]) {
        for element in other {
            self.insert(0, *element);
        }
    }

    fn reverse_extend(&mut self, other: &[T]) {
        for element in other.iter().rev() {
            self.push(*element);
        }
    }
}

fn chain<T: Copy + PartialEq>(chainable: &mut Vec<Vec<T>>) -> Vec<Vec<T>> {
    use Connection::*;

    let mut chains: Vec<Vec<T>> = vec![];
    for list in chainable {
        let first_elem = list.first();
        let last_elem = list.last();
        if let Some(connection) = chains.iter_mut().find_map(|chain| {
            let list_first = first_elem?;
            let list_last = last_elem?;
            let chain_first = chain.first()?;
            let chain_last = chain.last()?;
            if *chain_last == *list_first {
                Some(Tail(chain))
            } else if *chain_first == *list_last {
                Some(Head(chain))
            } else if *chain_last == *list_last {
                Some(ReverseTail(chain))
            } else if *chain_first == *list_first {
                Some(ReverseHead(chain))
            } else {
                None
            }
        }) {
            match connection {
                Tail(chain) => chain.extend(&list[1..]),
                Head(chain) => chain.prepend(&list[..list.len() - 1]),
                ReverseTail(chain) => chain.reverse_extend(&list[..list.len() - 1]),
                ReverseHead(chain) => chain.reverse_prepend(&list[1..]),
            }
        } else {
            chains.push(list.to_vec());
        }
    }
    chains
}

impl<T: Copy + PartialEq> Chainable<T> for Vec<Vec<T>> {
    fn merge(&mut self) {
        let mut vec_size;
        loop {
            vec_size = self.len();
            *self = chain(self);
            if self.len() == vec_size {
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tail() {
        let a = vec![1, 2, 3];
        let b = vec![3, 4, 5];
        let mut c = vec![a, b];
        c.merge();
        assert_eq!(c, vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn head() {
        let a = vec![3, 4, 5];
        let b = vec![1, 2, 3];
        let mut c = vec![a, b];
        c.merge();
        assert_eq!(c, vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn reverse_tail() {
        let a = vec![1, 2, 3];
        let b = vec![5, 4, 3];
        let mut c = vec![a, b];
        c.merge();
        assert_eq!(c, vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn reverse_head() {
        let a = vec![3, 4, 5];
        let b = vec![3, 2, 1];
        let mut c = vec![a, b];
        c.merge();
        assert_eq!(c, vec![vec![1, 2, 3, 4, 5]]);
    }

    #[test]
    fn disjointed() {
        let a = vec![5, 6, 7];
        let b = vec![1, 2, 3];
        let c = vec![3, 4, 5];
        let mut d = vec![a, b, c];
        d.merge();
        assert_eq!(d, vec![vec![1, 2, 3, 4, 5, 6, 7]]);
    }

    #[test]
    fn unrelated() {
        let a = vec![1, 2, 3];
        let b = vec![4, 5, 6];
        let mut c = vec![a, b];
        c.merge();
        assert_eq!(c, vec![vec![1, 2, 3], vec![4, 5, 6]]);
    }
}
