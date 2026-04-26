package handlers

import (
	"encoding/json"
	"net/http"
)

func JSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}

func OK(w http.ResponseWriter, v any) {
	JSON(w, http.StatusOK, v)
}

func Created(w http.ResponseWriter, v any) {
	JSON(w, http.StatusCreated, v)
}

func NoContent(w http.ResponseWriter) {
	w.WriteHeader(http.StatusNoContent)
}

func ErrResponse(w http.ResponseWriter, status int, msg string) {
	JSON(w, status, map[string]string{"error": msg})
}

func NotFound(w http.ResponseWriter) {
	ErrResponse(w, http.StatusNotFound, "not found")
}

func BadRequest(w http.ResponseWriter, msg string) {
	ErrResponse(w, http.StatusBadRequest, msg)
}

func InternalError(w http.ResponseWriter, err error) {
	ErrResponse(w, http.StatusInternalServerError, err.Error())
}
