use std::pin::Pin;

use futures_lite::future::Future;

pub struct UnsafeSend<T> {
    pub t: T,
}

unsafe impl<T> Send for UnsafeSend<T> {}

impl<T> UnsafeSend<T> {
    pub fn new(t: T) -> UnsafeSend<T> {
        UnsafeSend { t }
    }
}

pub struct UnsafeSendFut<T> {
    inner: Pin<Box<dyn Future<Output = T>>>,
}

impl<T> UnsafeSendFut<T> {
    pub fn new<F: Future<Output = T> + 'static>(inner: F) -> UnsafeSendFut<T> {
        UnsafeSendFut {
            inner: Box::pin(inner),
        }
    }
}

unsafe impl<T> Send for UnsafeSendFut<T> {}

impl<T> Future for UnsafeSendFut<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}
