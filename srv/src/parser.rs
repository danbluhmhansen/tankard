use nom::{
    bytes::complete::tag, character::complete::alphanumeric1, multi::separated_list0,
    sequence::preceded,
};

pub(crate) fn query_select(input: &str) -> nom::IResult<&str, Vec<&str>> {
    preceded(tag("select="), separated_list0(tag(","), alphanumeric1))(input)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::query_select;

    #[test]
    fn foo() -> Result<(), Box<dyn Error>> {
        let (_, select) = query_select("select=username,email")?;

        assert_eq!(vec!["username", "email"], select);

        Ok(())
    }
}
