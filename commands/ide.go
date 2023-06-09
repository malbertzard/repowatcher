package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
)

func OpenIDERepositories(repo *models.Repository, config *models.Config, receiver receiver.Receiver) {
	if repo != nil {
		repoPath := helpers.GetRepositoryPath(repo, config)
		cmd := exec.Command(config.EditCommand, repoPath)
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
		err := cmd.Run()
		if err != nil {
			fmt.Printf("Failed to open repository %s in IDE: %v\n", repo.Nickname, err)
			os.Exit(1) // Exit the program with an error status
		}
	} else {
		fmt.Println("Repository not found in config.")
	}
}
