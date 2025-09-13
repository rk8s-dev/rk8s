extern crate enquote;

#[test]
fn enquote() {
    assert_eq!(
        enquote::enquote('"', r#""Fran & Freddie's Diner	☺\""#),
        r#""\"Fran & Freddie's Diner	☺\\\"""#,
    );
    assert_eq!(enquote::enquote('"', ""), r#""""#);
    assert_eq!(enquote::enquote('"', r#"""#), r#""\"""#);

    assert_eq!(
        enquote::enquote('\'', r#""Fran & Freddie's Diner	☺\""#),
        r#"'"Fran & Freddie\'s Diner	☺\\"'"#,
    );
    assert_eq!(enquote::enquote('\'', ""), "''");
    assert_eq!(enquote::enquote('\'', "'"), r#"'\''"#);

    assert_eq!(enquote::enquote('`', ""), "``");
    assert_eq!(enquote::enquote('`', "`"), r#"`\``"#);
}

#[test]
fn unquote() {
    assert_eq!(
        enquote::unquote("").unwrap_err(),
        enquote::Error::NotEnoughChars,
    );
    assert_eq!(
        enquote::unquote("foobar").unwrap_err(),
        enquote::Error::UnrecognizedQuote,
    );
    assert_eq!(
        enquote::unquote("'foobar").unwrap_err(),
        enquote::Error::UnexpectedEOF,
    );
    assert_eq!(
        enquote::unquote("'foo'bar'").unwrap_err(),
        enquote::Error::IllegalChar,
    );
    assert_eq!(
        enquote::unquote("'foobar\\'").unwrap_err(),
        enquote::Error::UnexpectedEOF,
    );
    assert_eq!(
        enquote::unquote("'\\q'").unwrap_err(),
        enquote::Error::UnrecognizedEscape,
    );
    assert_eq!(
        enquote::unquote("'\\00'").unwrap_err(),
        enquote::Error::UnrecognizedEscape,
    );

    assert_eq!(
        enquote::unquote(r#""\"Fran & Freddie's Diner	☺\\\"""#).unwrap(),
        r#""Fran & Freddie's Diner	☺\""#,
    );
    assert_eq!(enquote::unquote(r#""""#).unwrap(), "");
    assert_eq!(enquote::unquote(r#""\"""#).unwrap(), r#"""#);

    assert_eq!(
        enquote::unquote(r#"'"Fran & Freddie\'s Diner	☺\\"'"#).unwrap(),
        r#""Fran & Freddie's Diner	☺\""#,
    );
    assert_eq!(enquote::unquote("''").unwrap(), "");
    assert_eq!(enquote::unquote(r#"'\''"#).unwrap(), "'");

    assert_eq!(enquote::unquote("``").unwrap(), "");
    assert_eq!(enquote::unquote(r#"`\``"#).unwrap(), "`");

    assert_eq!(enquote::unquote("'\\n'").unwrap(), "\n");
    assert_eq!(enquote::unquote("'\\101'").unwrap(), "A");
    assert_eq!(enquote::unquote("'\\x76'").unwrap(), "\x76");
    assert_eq!(enquote::unquote("'\\u2714'").unwrap(), "\u{2714}");
    assert_eq!(enquote::unquote("'\\U0001f427'").unwrap(), "\u{1f427}");
}
