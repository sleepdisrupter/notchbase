//! Nests BEGIN and END lines into components.

use super::lines::Property;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    pub kind: String,
    pub props: Vec<Property>,
    pub children: Vec<Component>,
}

impl Component {
    pub fn tree(props: Vec<Property>) -> Component {
        let mut stack = vec![Component::default()];
        for prop in props {
            match prop.name.as_str() {
                "BEGIN" => {
                    stack.push(Component { kind: prop.value.trim().to_ascii_uppercase(), ..Component::default() })
                }
                "END" if stack.len() > 1 => {
                    let done = stack.pop().unwrap_or_default();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(done);
                    }
                }
                "END" => {}
                _ => {
                    if let Some(current) = stack.last_mut() {
                        current.props.push(prop);
                    }
                }
            }
        }
        while stack.len() > 1 {
            let open = stack.pop().unwrap_or_default();
            if let Some(parent) = stack.last_mut() {
                parent.children.push(open);
            }
        }
        stack.pop().unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<&Property> {
        self.props.iter().find(|prop| prop.name == name)
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Property> {
        self.props.iter().filter(move |prop| prop.name == name)
    }

    pub fn text(&self, name: &str) -> String {
        self.get(name).map(Property::text).unwrap_or_default()
    }

    pub fn find<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Component> {
        self.children.iter().filter(move |child| child.kind == kind)
    }

    pub fn descendants<'a>(&'a self, kind: &'a str) -> Vec<&'a Component> {
        let mut found = Vec::new();
        for child in &self.children {
            if child.kind == kind {
                found.push(child);
            }
            found.extend(child.descendants(kind));
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::properties;
    use super::*;

    #[test]
    fn components_nest_and_keep_unclosed() {
        let text = "BEGIN:VCALENDAR\nX-WR-CALNAME:Work\nBEGIN:VEVENT\nSUMMARY:Standup\nBEGIN:VALARM\nACTION:DISPLAY\nEND:VALARM\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:Cut off";
        let root = Component::tree(properties(text));
        let calendar = root.find("VCALENDAR").next().unwrap();
        assert_eq!(calendar.text("X-WR-CALNAME"), "Work");
        let events: Vec<_> = calendar.find("VEVENT").collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].children[0].kind, "VALARM");
        assert_eq!(root.descendants("VEVENT").len(), 2);
        assert_eq!(events[1].text("SUMMARY"), "Cut off");
    }
}
