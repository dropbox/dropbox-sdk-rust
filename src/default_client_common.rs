macro_rules! impl_set_path_root {
    ($self:ident) => {
        /// Set a root which all subsequent paths are evaluated relative to.
        ///
        /// The default, if this function is not called, is to behave as if it was called with
        /// [`PathRoot::Home`](crate::common::PathRoot::Home).
        ///
        /// See <https://www.dropbox.com/developers/reference/path-root-header-modes> for more
        /// information.
        #[cfg(feature = "dbx_common")]
        pub fn set_path_root(&mut $self, path_root: &crate::common::PathRoot) {
            // Only way this can fail is if PathRoot::Other was specified, which is a programmer
            // error, so panic if that happens.
            $self.path_root = Some(serde_json::to_string(path_root).expect("invalid path root"));
        }
    }
}
pub(crate) use impl_set_path_root;
