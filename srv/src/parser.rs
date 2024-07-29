use std::str::FromStr;

use winnow::{
    combinator::{preceded, separated},
    stream::AsChar,
    token::take_while,
    PResult, Parser,
};

use crate::api::Select;

impl Select {
    fn parse(input: &mut &str) -> PResult<Self> {
        preceded(
            "select=",
            separated(
                0..,
                take_while(0.., |c: char| c.is_alphanum() || c == '_'),
                ",",
            )
            .map(|v: Vec<&str>| Self(v.iter().map(|s| s.to_string()).collect())),
        )
        .parse_next(input)
    }
}

impl FromStr for Select {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse.parse(s).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, str::FromStr};

    use crate::api::Select;

    #[test]
    fn single() -> Result<(), Box<dyn Error>> {
        let select = Select::from_str("select=username")?;

        assert_eq!(Select(vec!["username".to_string()]), select);

        Ok(())
    }

    #[test]
    fn multiple() -> Result<(), Box<dyn Error>> {
        let select = Select::from_str("select=username,email")?;

        assert_eq!(
            Select(vec!["username".to_string(), "email".to_string()]),
            select
        );

        Ok(())
    }
}
