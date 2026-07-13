use std::{any::Any, fmt, result};

/// Converts fallible operations into [`Err`] while preserving wraps and
/// attachments.
pub trait ResultExt<T> {
    /// Wraps error with `message` while retaining prior frames.
    #[track_caller]
    fn wrap(self, message: &'static str) -> Result<T>;

    /// Adds typed attachment to error value.
    fn attach<A>(self, attachment: A) -> Result<T>
    where
        A: Send + Sync + 'static;

    /// Adds printable attachment to error value.
    fn attach_printable<A>(self, attachment: A) -> Result<T>
    where
        A: fmt::Display + Send + Sync + 'static;

    /// Attaches `value` as labeled debug payload to error.
    fn debug<V>(self, label: &'static str, value: V) -> Result<T>
    where
        V: fmt::Debug + Send + Sync;
}

impl<T, E> ResultExt<T> for result::Result<T, E>
where
    E: fmt::Display + Send + Sync + 'static,
{
    #[track_caller]
    fn wrap(self, message: &'static str) -> Result<T> {
        let location = std::panic::Location::caller();
        self.map_err(|error| Err::from_error(error).wrap_at(message, location))
    }

    fn attach<A>(self, attachment: A) -> Result<T>
    where
        A: Send + Sync + 'static,
    {
        self.map_err(|error| Err::from_error(error).attach(attachment))
    }

    fn attach_printable<A>(self, attachment: A) -> Result<T>
    where
        A: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|error| Err::from_error(error).attach_printable(attachment))
    }

    fn debug<V>(self, label: &'static str, value: V) -> Result<T>
    where
        V: fmt::Debug + Send + Sync,
    {
        self.map_err(|error| Err::from_error(error).debug(label, value))
    }
}

/// Shared error type used across browser runtime.
pub struct Err {
    frames: Vec<ErrorFrame>,
}

enum Message {
    Static(&'static str),
    Owned(String),
}

impl Message {
    fn as_str(&self) -> &str {
        match self {
            Self::Static(message) => message,
            Self::Owned(message) => message,
        }
    }
}

struct ErrorFrame {
    message: Message,
    location: Option<&'static std::panic::Location<'static>>,
    attachments: Vec<Box<dyn Any + Send + Sync>>,
}

impl Err {
    fn fmt_frame(f: &mut fmt::Formatter<'_>, frame: &ErrorFrame, indent: &str) -> fmt::Result {
        match frame.location {
            Some(location) => writeln!(
                f,
                "{indent}{} (at {}:{})",
                frame.message.as_str(),
                location.file(),
                location.line()
            )?,
            None => writeln!(f, "{indent}{}", frame.message.as_str())?,
        }
        Self::fmt_attachments(f, &frame.attachments, &format!("{indent}  "))
    }

    fn fmt_attachments(
        f: &mut fmt::Formatter<'_>,
        attachments: &[Box<dyn Any + Send + Sync>],
        indent: &str,
    ) -> fmt::Result {
        for attachment in attachments {
            if let Some(info) = attachment.downcast_ref::<DebugInfo>() {
                writeln!(f, "{indent}- {info}")?;
                continue;
            }
            if let Some(info) = attachment.downcast_ref::<PrintableAttachment>() {
                writeln!(f, "{indent}- {info}")?;
            }
        }
        Ok(())
    }

    /// Creates new error from one static message.
    #[track_caller]
    pub fn new(message: &'static str) -> Self {
        Self::new_at(message, std::panic::Location::caller())
    }

    fn new_at(message: &'static str, location: &'static std::panic::Location<'static>) -> Self {
        Self {
            frames: vec![ErrorFrame {
                message: Message::Static(message),
                location: Some(location),
                attachments: Vec::new(),
            }],
        }
    }

    fn from_frame_message(message: String) -> Self {
        Self {
            frames: vec![ErrorFrame {
                message: Message::Owned(message),
                location: None,
                attachments: Vec::new(),
            }],
        }
    }

    /// Converts any compatible error into [`Err`].
    pub fn from_error<E>(error: E) -> Self
    where
        E: fmt::Display + Send + Sync + 'static,
    {
        let value: Box<dyn Any + Send + Sync> = Box::new(error);
        match value.downcast::<Self>() {
            Ok(error) => *error,
            Err(value) => {
                let error = *value
                    .downcast::<E>()
                    .expect("boxed error type changed unexpectedly");
                Self::from_frame_message(error.to_string())
            }
        }
    }

    /// Wraps error with new outer message while keeping prior frames.
    #[track_caller]
    pub fn wrap(self, message: &'static str) -> Self {
        self.wrap_at(message, std::panic::Location::caller())
    }

    fn wrap_at(
        mut self,
        message: &'static str,
        location: &'static std::panic::Location<'static>,
    ) -> Self {
        self.frames.push(ErrorFrame {
            message: Message::Static(message),
            location: Some(location),
            attachments: Vec::new(),
        });
        self
    }

    /// Adds typed attachment.
    pub fn attach<A>(mut self, attachment: A) -> Self
    where
        A: Send + Sync + 'static,
    {
        self.frames
            .last_mut()
            .expect("Err always contains at least one frame")
            .attachments
            .push(Box::new(attachment));
        self
    }

    /// Adds printable attachment.
    pub fn attach_printable<A>(self, attachment: A) -> Self
    where
        A: fmt::Display + Send + Sync + 'static,
    {
        self.attach(PrintableAttachment(attachment.to_string()))
    }

    /// Adds labeled debug attachment.
    pub fn debug<V>(self, label: &'static str, value: V) -> Self
    where
        V: fmt::Debug + Send + Sync,
    {
        self.attach(DebugInfo::new(label, value))
    }

    /// Returns first attachment matching `T`.
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.frames().find_map(|frame| frame.downcast_ref::<T>())
    }

    /// Returns iterator over frame metadata and attachments.
    pub fn frames(&self) -> Frames<'_> {
        Frames {
            frames: self.frames.iter().rev(),
            attachments: None,
            pending_frame: None,
        }
    }

    fn display_frame(&self) -> &ErrorFrame {
        self.frames
            .last()
            .expect("Err always contains at least one frame")
    }
}

impl fmt::Display for Err {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let frame = self.display_frame();
        match frame.location {
            Some(location) => write!(
                f,
                "{} (at {}:{})",
                frame.message.as_str(),
                location.file(),
                location.line()
            ),
            None => f.write_str(frame.message.as_str()),
        }
    }
}

impl fmt::Debug for Err {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let top = self
            .frames
            .last()
            .expect("Err always contains at least one frame");
        Self::fmt_frame(f, top, "")?;

        if self.frames.len() > 1 {
            writeln!(f, "Caused by:")?;
            for (index, frame) in self.frames.iter().rev().skip(1).enumerate() {
                write!(f, "    {index}: ")?;
                Self::fmt_frame(f, frame, "")?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for Err {}

/// Single yielded frame view inside one [`Err`].
pub struct Frame<'a> {
    message: &'a Message,
    location: Option<&'static std::panic::Location<'static>>,
    attachment: Option<&'a (dyn Any + Send + Sync)>,
}

impl<'a> Frame<'a> {
    /// Returns frame message.
    pub fn message(&self) -> &str {
        self.message.as_str()
    }

    /// Returns frame source file.
    pub fn file(&self) -> Option<&'static str> {
        self.location.map(std::panic::Location::file)
    }

    /// Returns frame source line.
    pub fn line(&self) -> Option<u32> {
        self.location.map(std::panic::Location::line)
    }

    /// Downcasts current frame attachment to `T`.
    pub fn downcast_ref<T>(&self) -> Option<&'a T>
    where
        T: Send + Sync + 'static,
    {
        self.attachment
            .and_then(|attachment| attachment.downcast_ref::<T>())
    }
}

/// Iterator over frames inside one [`Err`].
pub struct Frames<'a> {
    frames: std::iter::Rev<std::slice::Iter<'a, ErrorFrame>>,
    attachments: Option<(
        &'a ErrorFrame,
        std::iter::Rev<std::slice::Iter<'a, Box<dyn Any + Send + Sync>>>,
    )>,
    pending_frame: Option<&'a ErrorFrame>,
}

impl<'a> Iterator for Frames<'a> {
    type Item = Frame<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((frame, attachments)) = &mut self.attachments {
            if let Some(attachment) = attachments.next() {
                return Some(Frame {
                    message: &frame.message,
                    location: frame.location,
                    attachment: Some(attachment.as_ref()),
                });
            }
            self.attachments = None;
        }

        if let Some(frame) = self.pending_frame.take() {
            return Some(Frame {
                message: &frame.message,
                location: frame.location,
                attachment: None,
            });
        }

        let frame = self.frames.next()?;
        self.attachments = Some((frame, frame.attachments.iter().rev()));
        self.pending_frame = Some(frame);
        self.next()
    }
}

/// Shared result alias backed by [`Err`].
pub type Result<T> = result::Result<T, Err>;

#[derive(Debug)]
pub struct DebugInfo {
    label: &'static str,
    value: String,
}

impl DebugInfo {
    /// Creates one labeled debug attachment.
    pub fn new<V>(label: &'static str, value: V) -> Self
    where
        V: fmt::Debug,
    {
        Self {
            label,
            value: format!("{value:?}"),
        }
    }
}

impl fmt::Display for DebugInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.label, self.value)
    }
}

#[derive(Debug)]
struct PrintableAttachment(String);

impl fmt::Display for PrintableAttachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Err, ResultExt};

    #[test]
    fn wraps_error_and_preserves_context() {
        let error = std::result::Result::<(), _>::Err("database offline")
            .wrap("loading account")
            .debug("account_id", 42);

        let error = error.expect_err("operation should fail");
        let rendered = format!("{error:?}");

        assert!(rendered.contains("loading account"));
        assert!(rendered.contains("database offline"));
        assert!(rendered.contains("account_id: 42"));
        assert_eq!(error.frames().count(), 3);
    }

    #[test]
    fn typed_attachments_are_retrievable() {
        let error = Err::new("request failed").attach(42_u16);

        assert_eq!(error.downcast_ref::<u16>(), Some(&42));
        assert!(error.downcast_ref::<u32>().is_none());
    }

    #[test]
    fn frame_metadata_is_available() {
        let error = Err::new("bad input").wrap("parsing config");
        let frame = error
            .frames()
            .find(|frame| frame.message() == "parsing config")
            .expect("wrapped frame should exist");

        assert!(frame.file().is_some());
        assert!(frame.line().is_some());
    }
}
