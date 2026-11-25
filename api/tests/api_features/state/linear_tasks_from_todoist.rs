use universal_inbox::task::Task;

#[derive(Default)]
pub struct LinearTasksFromTodoist {
    pub others_task: Option<Task>,
}
