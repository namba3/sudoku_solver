use std::str::FromStr;

pub(super) fn parse_value<T: FromStr>(value: &str, name: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value for {name}"))
}

pub(super) fn next_value(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

pub(super) fn next_parsed<T: FromStr>(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, String> {
    parse_value(&next_value(args, name)?, name)
}
