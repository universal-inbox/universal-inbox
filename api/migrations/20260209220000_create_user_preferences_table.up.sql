CREATE TABLE user_preferences (
    user_id UUID NOT NULL PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    default_task_manager_provider_kind TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);
