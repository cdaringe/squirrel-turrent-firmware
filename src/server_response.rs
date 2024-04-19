use serde::ser::SerializeStruct;
use serde::Serialize;
use serde::Serializer;
use serde_json;

pub enum Response<T: Serialize, E: Serialize> {
    Ok(T),
    Error(E),
}

impl<T: Serialize> Response<T, ()> {
    pub fn ok(data: T) -> Self {
        Response::Ok(data)
    }
}

impl<E: Serialize> Response<(), E> {
    pub fn error(data: E) -> Self {
        Response::Error(data)
    }
}

impl<T: Serialize, E: Serialize> Response<T, E> {
    pub fn json(&self) -> serde_json::Result<String> {
        serde_json::to_string(&self)
    }
}

impl<T: Serialize, E: Serialize> Serialize for Response<T, E> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Response::Ok(t) => {
                let mut state = serializer.serialize_struct("Response", 2)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("data", t)?;
                state.end()
            }
            Response::Error(err) => {
                let mut state = serializer.serialize_struct("Response", 2)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("data", err)?;
                state.end()
            }
        }
    }
}
