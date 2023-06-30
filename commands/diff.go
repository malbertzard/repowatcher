package commands

import (
	"log"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
	"strings"
)

func ShowDiff(config *models.Config, nickname string, receiver receiver.Receiver, allReposFlag bool, showRemoteDiff bool) {
	if allReposFlag {
		for _, repo := range config.Repositories {
			showDiffForRepository(&repo, config, showRemoteDiff)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			showDiffForRepository(repo, config, showRemoteDiff)
		} else {
			log.Println("Repository not found in config.")
		}
	}
}

func showDiffForRepository(repo *models.Repository, config *models.Config, showRemoteDiff bool) {
	repoPath := helpers.GetRepositoryPath(repo, config)

	if showRemoteDiff {
		if err := helpers.RunCommand("git", "-C", repoPath, "fetch"); err != nil {
			log.Printf("Failed to fetch remote for repository %s: %v", repo.Nickname, err)
			return
		}

		output, err := helpers.RunCommandOutput("git", "-C", repoPath, "rev-parse", "--abbrev-ref", "HEAD")
		if err != nil {
			log.Printf("Failed to get current branch for repository %s: %v", repo.Nickname, err)
			return
		}
		branch := strings.TrimSpace(output)

		if err := helpers.RunCommand("git", "-C", repoPath, "diff", branch, "origin/"+branch); err != nil {
			log.Printf("Failed to show remote diff for repository %s: %v", repo.Nickname, err)
		}
	} else {
		if err := helpers.RunCommand("git", "-C", repoPath, "diff"); err != nil {
			log.Printf("Failed to show diff for repository %s: %v", repo.Nickname, err)
		}
	}
}
