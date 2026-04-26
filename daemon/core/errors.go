package core

import "errors"

var (
	ErrNotFound      = errors.New("not found")
	ErrAlreadyExists = errors.New("already exists")
	ErrNotRunning    = errors.New("service not running")
	ErrAlreadyRunning = errors.New("service already running")
)
