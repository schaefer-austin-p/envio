//! A mailbox: an optional display name and an email address.

/// One mailbox, as shown to the user and written into headers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

impl Address {
    /// An address with an optional name. A blank name counts as none, and
    /// control characters in a name become spaces.
    pub fn new(name: Option<&str>, email: &str) -> Self {
        let name = name.map(|n| {
            n.chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect::<String>()
        });
        let name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        Self {
            name: name.map(str::to_owned),
            email: email.trim().to_owned(),
        }
    }

    /// Reads `Name <email>`, `<email>` or a bare `email`. `None` when there
    /// is no `@` in the address part or it holds a control character,
    /// whitespace or an address special (`< > , ; : " ( ) [ ] \`).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (name, email) = match (text.rfind('<'), text.ends_with('>')) {
            (Some(open), true) => (
                Some(text[..open].trim().trim_matches('"')),
                &text[open + 1..text.len() - 1],
            ),
            _ => (None, text),
        };
        let email = email.trim();
        (email.contains('@')
            && !email.chars().any(|c| c.is_control() || c.is_whitespace())
            && !has_specials(email))
        .then(|| Self::new(name, email))
    }

    /// "Name <email>", or the bare email when there is no name.
    pub fn display(&self) -> String {
        match &self.name {
            Some(name) => format!("{name} <{}>", self.email),
            None => self.email.clone(),
        }
    }

    /// Whether this is the given email, ignoring ASCII case.
    pub fn is(&self, email: &str) -> bool {
        same_email(&self.email, email)
    }
}

/// Whether the text holds a character that ends or splits an address in a
/// header: `< > , ; : " ( ) [ ] \`.
pub(crate) fn has_specials(text: &str) -> bool {
    text.contains(is_special)
}

/// One of the characters [`has_specials`] looks for.
pub(crate) fn is_special(c: char) -> bool {
    matches!(
        c,
        '<' | '>' | ',' | ';' | ':' | '"' | '(' | ')' | '[' | ']' | '\\'
    )
}

/// Email equality as users expect it: ASCII case and outer spaces ignored.
pub fn same_email(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The mailboxes of a parsed header, groups flattened, empty ones skipped.
pub(crate) fn from_header(header: Option<&mail_parser::Address<'_>>) -> Vec<Address> {
    header
        .into_iter()
        .flat_map(|list| list.iter())
        .filter_map(|addr| {
            let email = addr.address.as_deref()?;
            (!email.trim().is_empty()).then(|| Address::new(addr.name.as_deref(), email))
        })
        .collect()
}

/// Removes later duplicates (by email, ignoring case), keeping order.
pub(crate) fn dedup(addresses: Vec<Address>) -> Vec<Address> {
    let mut out: Vec<Address> = Vec::with_capacity(addresses.len());
    for address in addresses {
        if !out.iter().any(|kept| kept.is(&address.email)) {
            out.push(address);
        }
    }
    out
}
