pub(super) struct Component {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub text: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/third_party.rs"));
