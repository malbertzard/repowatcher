package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
)

func ExecInRepositories(repo *models.Repository, commandArgs []string,config *models.Config) {
	if repo != nil {
        repoPath := helpers.GetRepositoryPath(repo, config)
		cmd := exec.Command(commandArgs[0], commandArgs[1:]...)
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
		cmd.Dir = repoPath
		err := cmd.Run()
		if err != nil {
			fmt.Printf("Failed to execute command in repository %s: %v\n", repo.Nickname, err)
		}
	} else {
		fmt.Println("Repository not found in config.")
	}
}
