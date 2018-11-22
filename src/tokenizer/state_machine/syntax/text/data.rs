define_state_group!(data_states_group = {

    data_state <-- ( enter_initial_text_parsing_mode; ) {
        b'<' => ( emit_chars; --> tag_open_state )
        eoc  => ( emit_chars; )
        eof  => ( emit_chars; emit_eof; )
        _    => ()
    }

});