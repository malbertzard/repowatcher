package models

type Repository struct {
	Nickname   string `yaml:"nickname"`
	FolderName string `yaml:"folderName"`
	URL        string `yaml:"url"`
	Sparse     bool   `yaml:"sparse"`
}
