package models

type Config struct {
	RootFolder   string       `yaml:"rootFolder"`
	EditCommand  string       `yaml:"editCommand"`
	Repositories []Repository`yaml:"repositories"`
}
