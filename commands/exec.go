package commands

import (
	"fmt"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
)

func ExecInRepositories(repo *models.Repository, commandArgs []string, config *models.Config, receiver receiver.Receiver) {
	if repo != nil {
		repoPath := helpers.GetRepositoryPath(repo, config)
		err := helpers.RunCommand(commandArgs[0], append(commandArgs[1:], repoPath)...)
		if err != nil {
			fmt.Printf("Failed to execute command in repository %s: %v\n", repo.Nickname, err)
		}
	} else {
		fmt.Println("Repository not found in config.")
	}
}
