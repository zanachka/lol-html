macro_rules! condition {
    ( | $self:tt |> appropriate_end_tag ) => {
        match $self.current_token {
            Some(ShallowToken::EndTag { name_hash, .. }) => {
                $self.last_start_tag_name_hash == name_hash
            }
            _ => unreachable!("End tag should exist at this point"),
        }
    };
}
