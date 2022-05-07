use async_graphql::{
    indexmap::IndexMap, registry::Registry, Context, EmptySubscription, InputType, Object, Scalar,
    Schema, SimpleObject,
};
use big_s::S;
use chrono::{DateTime, Utc};
use glob::glob;
use lol_html::rewrite_str;
use serde::{Deserialize, Serialize};
use slugify::slugify;
use std::{borrow::Cow, fs, path::PathBuf};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Serialize, Deserialize)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

async_graphql::scalar!(DateRange);

impl DateRange {
    fn within(&self, date: &str) -> bool {
        let start = DateTime::parse_from_rfc3339(&self.start).unwrap();
        let end = DateTime::parse_from_rfc3339(&self.end).unwrap();
        let date = DateTime::parse_from_rfc3339(date).unwrap();
        date >= start && date <= end
    }
}

#[derive(Debug, PartialEq, SimpleObject, Clone, Serialize, Deserialize)]
pub struct NoteDates {
    creation: String,
    last_modification: String,
}

#[derive(Serialize, Deserialize, Debug, SimpleObject, Clone)]
pub struct Note {
    #[serde(default)]
    tags: Vec<String>,

    #[serde(skip)]
    id: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    project: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    area: String,

    #[serde(default = "empty_dates")]
    date_of: NoteDates,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    url: String,

    #[serde(skip)]
    body: String,

    #[serde(skip)]
    title: String,
}

impl Note {
    fn satisfies(
        &self,
        area: &Option<String>,
        project: &Option<String>,
        tags: &Option<Vec<String>>,
        created: &Option<DateRange>,
        last_modified: &Option<DateRange>,
    ) -> bool {
        if let Some(area) = area {
            if &self.area != area {
                return false;
            }
        }

        if let Some(project) = project {
            if &self.project != project {
                return false;
            }
        }

        if let Some(tags) = tags {
            if !self.tags.iter().any(|tag| tags.contains(tag)) {
                return false;
            }
        }

        if let Some(created) = created {
            if !created.within(&self.date_of.creation) {
                return false;
            }
        }

        if let Some(last_modified) = last_modified {
            if !last_modified.within(&self.date_of.last_modification) {
                return false;
            }
        }

        true
    }
}

fn empty_dates() -> NoteDates {
    NoteDates {
        creation: S(""),
        last_modification: S(""),
    }
}

#[derive(Clone)]
pub struct Queyd {
    inside: PathBuf,
}

impl Queyd {
    pub fn new() -> Self {
        println!("Queyd::new()");
        let inside = dirs::home_dir().unwrap().join("ideas");
        println!("inside is {}", inside.display());
        Self { inside }
    }

    pub fn notes(
        &self,
        area: Option<String>,
        project: Option<String>,
        tags: Option<Vec<String>>,
        created: Option<DateRange>,
        last_modified: Option<DateRange>,
    ) -> Result<Vec<Note>> {
        println!("Queyd::notes()");
        let mut notes: Vec<Note> = vec![];

        let pattern = format!("{}/**/*.md", self.inside.display());
        println!("pattern: {:?}", pattern);

        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => match fs::read_to_string(&path) {
                    Ok(content) => {
                        if !content.starts_with("---\n") {
                            continue;
                        }

                        if let [_, metadata_raw, content_raw] =
                            content.splitn(3, "---\n").collect::<Vec<&str>>()[..]
                        {
                            let mut note: Note = serde_yaml::from_str(metadata_raw)?;
                            if !note.satisfies(&area, &project, &tags, &created, &last_modified) {
                                continue;
                            }
                            let html_raw = markdown::to_html(content_raw);

                            note.date_of = NoteDates {
                                creation: if note.date_of.creation.is_empty() {
                                    match fs::metadata(&path)?.created() {
                                        Ok(created) => {
                                            let date: DateTime<Utc> = created.into();
                                            date.to_rfc3339()
                                        }
                                        Err(e) => {
                                            println!(
                                                "Couldn't get creation time for {:?}: {:?}",
                                                path, e
                                            );
                                            S("")
                                        }
                                    }
                                } else {
                                    note.date_of.creation
                                },
                                last_modification: if note.date_of.last_modification.is_empty() {
                                    match fs::metadata(&path)?.modified() {
                                        Ok(modified) => {
                                            let date: DateTime<Utc> = modified.into();
                                            date.to_rfc3339()
                                        }
                                        Err(e) => {
                                            println!(
                                                "Couldn't get modification time for {:?}: {:?}",
                                                path, e
                                            );
                                            S("")
                                        }
                                    }
                                } else {
                                    note.date_of.last_modification
                                },
                            };

                            note.id = path
                                .strip_prefix(&self.inside)
                                .expect("Failed to get file stem")
                                .with_extension("")
                                .to_str()
                                .unwrap()
                                .to_string();

                            note.body = match rewrite_str(
                                &html_raw,
                                lol_html::RewriteStrSettings {
                                    element_content_handlers: vec![lol_html::element!(
                                        "h1",
                                        |h1| {
                                            h1.remove();
                                            Ok(())
                                        }
                                    )],
                                    ..Default::default()
                                },
                            ) {
                                Ok(html) => html,
                                Err(e) => {
                                    println!("While processing markdown body: {:?}", e);
                                    html_raw.clone()
                                }
                            }
                            .trim()
                            .trim_matches('\n')
                            .to_string();

                            note.title = {
                                let html = tl::parse(&html_raw, tl::ParserOptions::default())?;
                                match html.query_selector("h1") {
                                    None => S(""),
                                    Some(mut h1s) => match h1s.next() {
                                        None => S(""),
                                        Some(h1) => h1
                                            .get(html.parser())
                                            .unwrap()
                                            .as_tag()
                                            .unwrap()
                                            .inner_text(html.parser())
                                            .into_owned(),
                                    },
                                }
                            };

                            notes.push(note);
                        } else {
                            println!("Content has no metadata: {}", content);
                        }
                    }
                    Err(e) => println!("{:?}", e),
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(notes)
    }

    pub fn add_note(&self, note: &Note) -> Result<()> {
        // XXX for some reason, using self.inside.join() doesn't work here
        let filepath = format!("{}/{}", self.inside.display(), (note.id.to_owned() + ".md"));
        println!("Queyd::add_note({:?})", filepath);
        // TODO this should be run, but it doesn't work (see XXX above)
        // fs::create_dir_all(filepath.parent().unwrap())?;
        fs::write(
            filepath,
            format!(
                "{}\n---\n# {}\n\n{}",
                serde_yaml::to_string(note)?,
                note.title,
                note.body
            ),
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        println!("Queyd::delete_note()");
        let filepath = self.inside.join(id.to_owned() + ".md");
        if !filepath.starts_with(&self.inside) {
            return Err("Can't delete file outside of Queyd's directory".into());
        }
        fs::remove_file(filepath)?;
        Ok(())
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn note<'a>(&self, ctx: &Context<'a>, id: String) -> Option<Note> {
        let queyd = ctx.data_unchecked::<Queyd>();
        queyd
            .notes(None, None, None, None, None)
            .expect("Failed to get notes")
            .iter()
            .find(|note| note.id == id)
            .map(|note| note.clone())
    }

    async fn notes<'a>(
        &self,
        ctx: &Context<'a>,
        area: Option<String>,
        project: Option<String>,
        tags: Option<Vec<String>>,
        created: Option<DateRange>,
        last_modified: Option<DateRange>,
    ) -> Vec<Note> {
        let queyd = ctx.data::<Queyd>().unwrap();
        queyd
            .notes(area, project, tags, created, last_modified)
            .expect("Failed to get notes")
            .iter()
            .map(|note| note.clone())
            .collect()
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn add(
        &self,
        ctx: &Context<'_>,
        title: String,
        project: Option<String>,
        area: Option<String>,
        body: String,
        tags: Option<Vec<String>>,
        id: Option<String>,
    ) -> Note {
        let queyd = ctx.data_unchecked::<Queyd>().clone();
        let note = Note {
            body,
            title: title.clone(),
            project: project.clone().unwrap_or_default(),
            tags: tags.unwrap_or(vec![]),
            area: area.unwrap_or_default(),
            id: if id.clone().unwrap_or_default().is_empty() {
                slugify!(&project.unwrap_or_default()) + "/" + &slugify!(&title)
            } else {
                id.unwrap_or_default()
            },
            url: S(""),
            date_of: NoteDates {
                creation: Utc::now().to_rfc3339(),
                last_modification: Utc::now().to_rfc3339(),
            },
        };
        queyd.add_note(&note).expect("Failed to add note");
        note
    }

    async fn delete(&self, ctx: &Context<'_>, id: String) -> bool {
        let queyd = ctx.data_unchecked::<Queyd>().clone();
        queyd.delete_note(&id).is_ok()
    }

    async fn edit(
        &self,
        ctx: &Context<'_>,
        id: String,
        title: Option<String>,
        body: Option<String>,
        project: Option<String>,
        area: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<Note> {
        let queyd = ctx.data_unchecked::<Queyd>().clone();
        let notes = queyd
            .notes(None, None, None, None, None)
            .expect("Couldn't load notes");
        let note = notes.iter().find(|note| note.id == id);
        match note {
            Some(note) => {
                let mut new_note = Note {
                    date_of: NoteDates {
                        last_modification: Utc::now().to_rfc3339(),
                        ..note.clone().date_of
                    },
                    ..note.clone()
                };
                if let Some(new_title) = title {
                    new_note.title = new_title
                }
                if let Some(new_body) = body {
                    new_note.body = new_body
                }
                if let Some(new_tags) = tags {
                    new_note.tags = new_tags
                }
                if let Some(new_project) = project {
                    new_note.project = new_project
                }
                if let Some(new_area) = area {
                    new_note.area = new_area
                }
                queyd.delete_note(&note.id)?;
                queyd.add_note(&new_note)?;
                Ok(new_note)
            }
            None => Err(format!("Cannot find note with id {}", id).into()),
        }
    }

    async fn archive(&self, ctx: &Context<'_>, id: String) -> Result<Note> {
        self.edit(ctx, id, None, None, None, Some(S("archive")), None)
            .await
    }
}

pub type QueydSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;
