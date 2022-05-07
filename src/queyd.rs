use async_graphql::*;
use big_s::S;
use chrono::{DateTime, Utc};
use slugify::slugify;
use uuid::Uuid;

#[derive(SimpleObject, Clone)]
struct NoteDates {
    creation: String,
    last_modification: String,
}

#[derive(SimpleObject, Clone)]
struct Note {
    title: String,
    body: String,
    tags: Vec<String>,
    id: String,
    uuid: String,
    date_of: NoteDates,
}

#[derive(Clone)]
pub struct Queyd {
    notes: Vec<Note>,
}

impl Queyd {
    pub fn new() -> Self {
        let notes = vec![Note {
            title: S("Hello, world!"),
            body: S("This is a test note."),
            date_of: NoteDates {
                creation: S("2020-01-01"),
                last_modification: S("2020-01-01"),
            },
            tags: vec![S("test"), S("note")],
            id: S("hello-world"),
            uuid: S("ogirjoigjreoigrjojgorjgoirjgorjgerjo"),
        }];
        Self { notes }
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn note<'a>(&self, ctx: &Context<'a>, id: String) -> Option<Note> {
        let queyd = ctx.data::<Queyd>().unwrap();

        queyd
            .notes
            .iter()
            .find(|note| note.id == id)
            .map(|note| note.clone())
    }

    async fn notes<'a>(&self, ctx: &Context<'a>) -> Vec<Note> {
        let queyd = ctx.data::<Queyd>().unwrap();

        queyd.notes.iter().map(|note| note.clone()).collect()
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn add(&self, ctx: &Context<'_>, title: String, body: String, tags: Vec<String>) -> Note {
        let mut queyd = ctx.data_unchecked::<Queyd>().clone();
        let id = slugify!(&title);
        let note = Note {
            title,
            body,
            tags,
            id,
            uuid: Uuid::new_v4().to_string(),
            date_of: NoteDates {
                creation: Utc::now().to_rfc3339(),
                last_modification: Utc::now().to_rfc3339(),
            },
        };
        queyd.notes.push(note.clone());
        note
    }

    async fn delete(&self, ctx: &Context<'_>, id: String) -> bool {
        let mut queyd = ctx.data_unchecked::<Queyd>().clone();
        queyd.notes.retain(|note| note.id != id);
        true
    }

    async fn edit(
        &self,
        ctx: &Context<'_>,
        id: String,
        title: String,
        body: String,
        tags: Vec<String>,
    ) -> Result<Note> {
        let mut queyd = ctx.data_unchecked::<Queyd>().clone();
        let note = queyd.notes.iter_mut().find(|note| note.id == id);
        match note {
            Some(note) => {
                note.title = title;
                note.body = body;
                note.tags = tags;
                note.date_of.last_modification = Utc::now().to_rfc3339();
                Ok(note.clone())
            }
            None => Err(format!("Cannot find note with id {}", id).into()),
        }
    }
}

pub type QueydSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;
