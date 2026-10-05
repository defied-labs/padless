use std::collections::BTreeMap;

use crate::code::Code;
use crate::table::Table;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolver {
    table: Table,
    aliases: BTreeMap<Code, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution<'a> {
    Alias(&'a str),
    Table(char),
}

impl Resolution<'_> {
    #[must_use]
    pub fn to_text(self) -> String {
        match self {
            Self::Alias(text) => text.to_owned(),
            Self::Table(c) => c.to_string(),
        }
    }
}

impl Resolver {
    #[must_use]
    pub fn new(table: Table, aliases: BTreeMap<Code, String>) -> Self {
        Self { table, aliases }
    }

    #[must_use]
    pub fn resolve(&self, code: &Code) -> Option<Resolution<'_>> {
        self.aliases
            .get(code)
            .map(|text| Resolution::Alias(text))
            .or_else(|| self.table.resolve(code).map(Resolution::Table))
    }

    #[must_use]
    pub fn table(&self) -> Table {
        self.table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> Code {
        text.parse().unwrap()
    }

    fn resolver_with(table: Table, aliases: &[(&str, &str)]) -> Resolver {
        let aliases = aliases
            .iter()
            .map(|(key, value)| (code(key), (*value).to_owned()))
            .collect();
        Resolver::new(table, aliases)
    }

    #[test]
    fn falls_back_to_table() {
        let resolver = resolver_with(Table::Windows, &[]);
        assert_eq!(
            resolver.resolve(&code("0233")),
            Some(Resolution::Table('\u{00E9}'))
        );
        assert_eq!(resolver.resolve(&code("0")), None);
    }

    #[test]
    fn aliases_take_precedence_over_table() {
        let resolver = resolver_with(Table::Windows, &[("65", "alpha")]);
        assert_eq!(
            resolver.resolve(&code("65")),
            Some(Resolution::Alias("alpha"))
        );
        assert_eq!(resolver.resolve(&code("065")), Some(Resolution::Table('A')));
    }

    #[test]
    fn aliases_fill_codes_the_table_rejects() {
        let resolver = resolver_with(Table::Unicode, &[("0", "zero"), ("55296", "\u{1F600}")]);
        assert_eq!(
            resolver.resolve(&code("0")),
            Some(Resolution::Alias("zero"))
        );
        assert_eq!(
            resolver.resolve(&code("55296")).map(Resolution::to_text),
            Some("\u{1F600}".to_owned())
        );
    }
}
