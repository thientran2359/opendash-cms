use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedMediaIds {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub file_ids: BTreeSet<String>,
    pub any_ids: BTreeSet<String>,
}

impl CollectedMediaIds {
    pub fn all_ids(&self) -> BTreeSet<String> {
        let mut all = BTreeSet::new();
        all.extend(self.image_ids.iter().cloned());
        all.extend(self.video_ids.iter().cloned());
        all.extend(self.file_ids.iter().cloned());
        all.extend(self.any_ids.iter().cloned());
        all
    }
}

pub(super) fn push_collected_media(out: &mut CollectedMediaIds, kind: MediaFieldKind, id: &str) {
    match kind {
        MediaFieldKind::Image => {
            out.image_ids.insert(id.to_owned());
        }
        MediaFieldKind::Video => {
            out.video_ids.insert(id.to_owned());
        }
        MediaFieldKind::File => {
            out.file_ids.insert(id.to_owned());
        }
        MediaFieldKind::Any => {
            out.any_ids.insert(id.to_owned());
        }
    }
}
