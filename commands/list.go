package commands

import (
	"fmt"
	"repo-watch/models"
)

func ListRepositories(config *models.Config) {
	for _, repo := range config.Repositories {
		fmt.Printf("Nickname: %s\n", repo.Nickname)
		fmt.Printf("URL: %s\n", repo.URL)
		fmt.Printf("Folder Name: %s\n", repo.FolderName)
		fmt.Println("----------------------")
	}
}
