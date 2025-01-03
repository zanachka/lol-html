use lol_html::html_content::ContentType;
use lol_html::{element, Settings};

define_group!(
    "Rewriting",
    [
        (
            "Modification of tags of an element with lots of content",
            Settings {
                element_content_handlers: vec![element!("body", |el| {
                    el.set_tag_name("body1").unwrap();
                    el.after("test", ContentType::Text);

                    Ok(())
                })],
                ..Settings::new()
            }
        ),
        (
            "Remove content of an element",
            Settings {
                element_content_handlers: vec![element!("ul", |el| {
                    el.set_inner_content("", ContentType::Text);

                    Ok(())
                })],
                ..Settings::new()
            }
        )
    ]
);
