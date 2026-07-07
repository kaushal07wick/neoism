//! Pure permission queue and choice policy shared by native and wasm panes.

use std::collections::VecDeque;

pub const VISUAL_SELECTION_ORDER: [usize; 3] = [1, 0, 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionQueueAction {
    MadeCurrent,
    Queued,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionReplyStart {
    NoCurrent,
    AlreadyResponding,
    MissingId,
    Ready { id: String },
}

pub fn rect_contains(rect: [f32; 4], x: f32, y: f32) -> bool {
    x >= rect[0] && x <= rect[0] + rect[2] && y >= rect[1] && y <= rect[1] + rect[3]
}

pub fn choice_at<T: Copy>(
    choices: impl IntoIterator<Item = (T, [f32; 4])>,
    x: f32,
    y: f32,
) -> Option<T> {
    choices
        .into_iter()
        .find(|(_, rect)| rect_contains(*rect, x, y))
        .map(|(choice, _)| choice)
}

pub fn move_selected_index(selected: usize, delta: isize) -> usize {
    let selected_position = VISUAL_SELECTION_ORDER
        .iter()
        .position(|choice| *choice == selected)
        .unwrap_or(1) as isize;
    let next = (selected_position + delta)
        .rem_euclid(VISUAL_SELECTION_ORDER.len() as isize) as usize;
    VISUAL_SELECTION_ORDER[next]
}

pub fn selected_reply(selected: usize) -> &'static str {
    match selected {
        1 => "always",
        2 => "reject",
        _ => "once",
    }
}

pub fn permission_is_pending<T>(
    current: &Option<T>,
    queue: &VecDeque<T>,
    id: &str,
    id_of: impl Fn(&T) -> &str,
) -> bool {
    current
        .as_ref()
        .is_some_and(|permission| id_of(permission) == id)
        || queue.iter().any(|permission| id_of(permission) == id)
}

pub fn enqueue_permission<T>(
    current: &mut Option<T>,
    queue: &mut VecDeque<T>,
    permission: T,
    id_of: impl Fn(&T) -> &str,
) -> PermissionQueueAction {
    if current.is_none() {
        *current = Some(permission);
        return PermissionQueueAction::MadeCurrent;
    }
    let id = id_of(&permission).to_string();
    if permission_is_pending(current, queue, &id, id_of) {
        PermissionQueueAction::Duplicate
    } else {
        queue.push_back(permission);
        PermissionQueueAction::Queued
    }
}

pub fn remove_permission<T>(
    current: &mut Option<T>,
    queue: &mut VecDeque<T>,
    request_id: &str,
    id_of: impl Fn(&T) -> &str,
) -> bool {
    let mut changed = false;
    if current
        .as_ref()
        .is_some_and(|permission| id_of(permission) == request_id)
    {
        *current = queue.pop_front();
        changed = true;
    }
    let before = queue.len();
    queue.retain(|permission| id_of(permission) != request_id);
    changed || before != queue.len()
}

pub fn clear_current_permission<T>(current: &mut Option<T>, queue: &mut VecDeque<T>) {
    *current = queue.pop_front();
}

pub fn start_reply<T>(
    current: &mut Option<T>,
    id_of: impl Fn(&T) -> &str,
    is_responding: impl Fn(&T) -> bool,
    set_responding: impl Fn(&mut T, bool),
) -> PermissionReplyStart {
    let Some(permission) = current.as_mut() else {
        return PermissionReplyStart::NoCurrent;
    };
    if is_responding(permission) {
        return PermissionReplyStart::AlreadyResponding;
    }
    set_responding(permission, true);
    let id = id_of(permission).to_string();
    if id.is_empty() {
        set_responding(permission, false);
        PermissionReplyStart::MissingId
    } else {
        PermissionReplyStart::Ready { id }
    }
}

pub fn fail_reply<T>(
    current: &mut Option<T>,
    id: &str,
    id_of: impl Fn(&T) -> &str,
    set_responding: impl Fn(&mut T, bool),
) -> bool {
    let Some(permission) = current
        .as_mut()
        .filter(|permission| id_of(permission) == id)
    else {
        return false;
    };
    set_responding(permission, false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Permission {
        id: &'static str,
        responding: bool,
    }

    fn id_of(permission: &Permission) -> &str {
        permission.id
    }

    fn responding(permission: &Permission) -> bool {
        permission.responding
    }

    fn set_responding(permission: &mut Permission, responding: bool) {
        permission.responding = responding;
    }

    #[test]
    fn moves_selection_in_visual_order() {
        assert_eq!(move_selected_index(0, 1), 2);
        assert_eq!(move_selected_index(2, 1), 1);
        assert_eq!(move_selected_index(1, 1), 0);
        assert_eq!(move_selected_index(0, -1), 1);
    }

    #[test]
    fn queue_dedupes_by_permission_id() {
        let mut current = None;
        let mut queue = VecDeque::new();
        assert_eq!(
            enqueue_permission(
                &mut current,
                &mut queue,
                Permission {
                    id: "perm-1",
                    responding: false
                },
                id_of
            ),
            PermissionQueueAction::MadeCurrent
        );
        assert_eq!(
            enqueue_permission(
                &mut current,
                &mut queue,
                Permission {
                    id: "perm-1",
                    responding: false
                },
                id_of
            ),
            PermissionQueueAction::Duplicate
        );
        assert_eq!(
            enqueue_permission(
                &mut current,
                &mut queue,
                Permission {
                    id: "perm-2",
                    responding: false
                },
                id_of
            ),
            PermissionQueueAction::Queued
        );
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn remove_permission_advances_current() {
        let mut current = Some(Permission {
            id: "perm-1",
            responding: false,
        });
        let mut queue = VecDeque::from([Permission {
            id: "perm-2",
            responding: false,
        }]);
        assert!(remove_permission(&mut current, &mut queue, "perm-1", id_of));
        assert_eq!(current.unwrap().id, "perm-2");
        assert!(queue.is_empty());
    }

    #[test]
    fn start_reply_marks_current_responding() {
        let mut current = Some(Permission {
            id: "perm-1",
            responding: false,
        });
        assert_eq!(
            start_reply(&mut current, id_of, responding, set_responding),
            PermissionReplyStart::Ready {
                id: "perm-1".to_string()
            }
        );
        assert!(current.unwrap().responding);
    }
}
