use std::rc::Rc;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Types {
    Single(Rc<str>),
    Union(Vec<Types>),
    Collection(Rc<str>, Box<Types>),
}
pub const ANY: &str = "any";
impl Types {
    pub fn single(name: &str) -> Types {
        Types::Single(crate::runtime::world::intern_type(name))
    }

    pub fn create(list: &[Types]) -> Types {
        let mut members: Vec<Types> = Vec::new();
        for t in list {
            match t {
                Types::Union(inner) => {
                    for item in inner {
                        push_member(item, &mut members);
                    }
                }
                _ => push_member(t, &mut members),
            }
        }
        if members.iter().any(|t| matches!(t, Types::Single(n) if &**n == ANY)) {
            return Types::Single(crate::runtime::world::intern_type(ANY));
        }
        if members.is_empty() {
            return Types::Single(crate::runtime::world::intern_type("none"));
        }
        if members.len() == 1 {
            return members.remove(0);
        }
        members.sort_by(|a, b| a.display().cmp(&b.display()));
        Types::Union(members)
    }

    pub fn contains(&self, other: &Types) -> bool {
        match (self, other) {
            (Types::Single(a), Types::Single(b)) => a == b,
            (Types::Union(v), t) => v.iter().any(|m| m == t),
            (Types::Collection(kind_a, item_a), Types::Collection(kind_b, item_b)) => {
                kind_a == kind_b && item_a == item_b
            }
            _ => false,
        }
    }

    pub fn name(&self) -> Option<Rc<str>> {
        match self {
            Types::Single(n) => Some(n.clone()),
            _ => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Types::Single(n) => n.to_string(),
            Types::Union(v) => v
                .iter()
                .map(|t| t.display())
                .collect::<Vec<_>>()
                .join(" | "),
            Types::Collection(kind, item) => format!("{kind}[{}]", item.display()),
        }
    }
}
fn push_member(t: &Types, members: &mut Vec<Types>) {
    if !members.contains(t) {
        members.push(t.clone());
    }
}
