package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
)

func CloneRepositories(config *models.Config, nickname string, receiver receiver.Receiver, allReposFlag bool) {
	if allReposFlag {
		for _, repo := range config.Repositories {
			pullChangesForRepository(&repo, config)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			fetchRepository(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
}

func cloneRepository(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmdArgs := []string{"clone"}
	if repo.Sparse {
		cmdArgs = append(cmdArgs, "--sparse")
	}
	cmdArgs = append(cmdArgs, repo.URL, repoPath)
	cmd := exec.Command("git", cmdArgs...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to clone repository %s: %v\n", repo.Nickname, err)
	}
}
