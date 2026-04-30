package core

import "sync"

// TaskState describes the lifecycle of a background operation.
type TaskState string

const (
	TaskStatePending TaskState = "pending"
	TaskStateRunning TaskState = "running"
	TaskStateDone    TaskState = "done"
	TaskStateError   TaskState = "error"
)

// AsyncTask tracks a single background operation. Methods are safe for
// concurrent use; callers in other packages use Set/Fail to update state.
type AsyncTask struct {
	State   TaskState `json:"state"`
	Message string    `json:"message"`
	Error   string    `json:"error,omitempty"`
	mu      sync.Mutex
}

func (t *AsyncTask) Set(state TaskState, msg string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.State = state
	t.Message = msg
}

func (t *AsyncTask) Fail(err error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.State = TaskStateError
	t.Error = err.Error()
}
