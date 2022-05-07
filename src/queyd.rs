use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use big_s::S;
use chrono::{DateTime, Utc};
use glob::glob;
use lol_html::rewrite_str;
use serde::{Deserialize, Serialize};
use slugify::slugify;
use std::{fs, path::PathBuf};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
    #[serde(default)]
    project: String,
    #[serde(default = "empty_dates")]
    date_of: NoteDates,
    #[serde(default)]
    url: String,
    #[serde(skip)]
    body: String,
    #[serde(skip)]
    title: String,
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
        Self {
            inside: dirs::home_dir().unwrap().join("ideas"),
        }
    }

    pub fn notes(&self) -> Result<Vec<Note>> {
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
                            let html_raw = markdown::to_html(content_raw);

                            note.date_of = NoteDates {
                                creation: if note.date_of.creation.is_empty() {
                                    let creation: DateTime<Utc> =
                                        fs::metadata(&path)?.created()?.into();
                                    creation.to_rfc3339()
                                } else {
                                    note.date_of.creation
                                },
                                last_modification: if note.date_of.last_modification.is_empty() {
                                    let creation: DateTime<Utc> =
                                        fs::metadata(&path)?.modified()?.into();
                                    creation.to_rfc3339()
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
        println!("Queyd::add_note()");
        let filepath = self.inside.join(note.id.to_owned() + ".md");
        fs::create_dir_all(filepath.parent().unwrap())?;
        fs::write(
            filepath,
            format!(
                "---\n{}\n---\n# {}\n\n{}",
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
            .notes()
            .expect("Failed to get notes")
            .iter()
            .find(|note| note.id == id)
            .map(|note| note.clone())
    }

    async fn notes<'a>(&self, ctx: &Context<'a>) -> Vec<Note> {
        let queyd = ctx.data::<Queyd>().unwrap();
        queyd
            .notes()
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
        id: String,
        title: String,
        project: String,
        body: String,
        tags: Vec<String>,
    ) -> Note {
        let queyd = ctx.data_unchecked::<Queyd>().clone();
        let note = Note {
            title: title.clone(),
            body,
            project: project.clone(),
            tags,
            id: if id.is_empty() {
                slugify!(&project) + "/" + &slugify!(&title)
            } else {
                id
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
        title: String,
        body: String,
        tags: Vec<String>,
    ) -> Result<Note> {
        let queyd = ctx.data_unchecked::<Queyd>().clone();
        let notes = queyd.notes().expect("Couldn't load notes");
        let note = notes.iter().find(|note| note.id == id);
        match note {
            Some(note) => {
                let new_note = Note {
                    title,
                    body,
                    tags,
                    date_of: NoteDates {
                        last_modification: Utc::now().to_rfc3339(),
                        ..note.clone().date_of
                    },
                    ..note.clone()
                };
                queyd.delete_note(&note.id)?;
                queyd.add_note(&new_note)?;
                Ok(new_note)
            }
            None => Err(format!("Cannot find note with id {}", id).into()),
        }
    }
}

pub type QueydSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;
