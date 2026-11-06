"""PDF <-> Word conversion engine bundled with Minos."""


class EngineError(Exception):
    """A failure with a code the app can translate (see `ErrorCode` in error.rs)."""

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
        self.message = message
