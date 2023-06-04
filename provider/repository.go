package provider

//TODO Git Provider

type RepoStorage interface {
	PullRepos() error
	DiffAllRepos() error
	DiffRepo(string) error
}

type GitRepoStorage struct {
	basePath string
}

func (g *GitRepoStorage) PullRepos() error {
	return nil
}
