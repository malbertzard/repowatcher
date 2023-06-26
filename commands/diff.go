package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
)

func DiffRepositories(config *models.Config, nickname string, receiver receiver.Receiver, allReposFlag bool)  {
	if allReposFlag {
		for _, repo := range config.Repositories {
			showRepositoryDiff(&repo, config)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			showRepositoryDiff(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
}

func showRepositoryDiff(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "diff")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to show diff for repository %s: %v\n", repo.Nickname, err)
	}
}

