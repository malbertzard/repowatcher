package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
)

func PullRepositories(config *models.Config, nickname string, allReposFlag bool)  {
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

func pullChangesForRepository(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "pull", "--all")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to pull changes for repository %s: %v\n", repo.Nickname, err)
	}
}
